from __future__ import annotations

import hashlib
import json
import re
from typing import Any


# This policy is embedded into the application package. Its pinned digest makes
# accidental or out-of-band replacement detectable; release signing protects
# both the policy and the verifier as one unit.
COMPATIBILITY_MANIFEST: dict[str, Any] = {
    "schema": 1,
    "policy_version": "2026-07-14",
    "components": {
        "xray": {"minimum_version": "25.12.8"},
        "singbox": {"minimum_version": "1.12.0"},
    },
    "geodata": {
        "required_geosite": ["ru-blocked", "category-ru"],
        "required_geoip": ["ru-blocked", "ru-blocked-community", "ru"],
    },
}
_TRUSTED_MANIFEST_SHA256 = "eb221483ad75554d699f8f48a6bcc11bfe13410b21788dd6da49200d0ecccaa3"
_VERSION_RE = re.compile(r"(\d+)\.(\d+)\.(\d+)")


def verify_compatibility_manifest() -> None:
    canonical = json.dumps(
        COMPATIBILITY_MANIFEST,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=True,
    ).encode("ascii")
    actual = hashlib.sha256(canonical).hexdigest()
    if actual != _TRUSTED_MANIFEST_SHA256:
        raise RuntimeError("манифест совместимости Lumen поврежден или изменен")


def _version_tuple(value: str) -> tuple[int, int, int] | None:
    match = _VERSION_RE.search(str(value or ""))
    if not match:
        return None
    return tuple(int(part) for part in match.groups())


def ensure_component_compatible(component: str, version: str) -> None:
    verify_compatibility_manifest()
    policy = COMPATIBILITY_MANIFEST["components"].get(component)
    if not isinstance(policy, dict):
        raise RuntimeError(f"неизвестный компонент в манифесте совместимости: {component}")
    current = _version_tuple(version)
    minimum_text = str(policy.get("minimum_version") or "")
    minimum = _version_tuple(minimum_text)
    if current is None:
        raise RuntimeError(f"не удалось определить версию {component}")
    if minimum is not None and current < minimum:
        raise RuntimeError(
            f"{component} {version} несовместим с этой версией Lumen; требуется {minimum_text} или новее"
        )


def required_geodata_codes() -> tuple[list[str], list[str]]:
    verify_compatibility_manifest()
    policy = COMPATIBILITY_MANIFEST["geodata"]
    return list(policy["required_geosite"]), list(policy["required_geoip"])
