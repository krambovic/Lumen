from __future__ import annotations

import pytest

from xray_fluent.component_compatibility import (
    ensure_component_compatible,
    required_geodata_codes,
    verify_compatibility_manifest,
)


def test_embedded_compatibility_manifest_is_intact() -> None:
    verify_compatibility_manifest()
    geosite, geoip = required_geodata_codes()
    assert "ru-blocked" in geosite
    assert "ru-blocked-community" in geoip


def test_component_minimum_version_is_enforced() -> None:
    ensure_component_compatible("xray", "Xray 26.7.11")
    with pytest.raises(RuntimeError, match="требуется"):
        ensure_component_compatible("singbox", "sing-box version 1.11.0")
