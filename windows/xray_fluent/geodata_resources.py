from __future__ import annotations

from pathlib import Path

from .constants import XRAY_PATH_DEFAULT


RUNETFREEDOM_RULES_BASE_URL = (
    "https://github.com/runetfreedom/russia-v2ray-rules-dat/releases/latest/download"
)
SINGBOX_RULES_ARCHIVE_URL = f"{RUNETFREEDOM_RULES_BASE_URL}/sing-box.zip"
SINGBOX_RULE_SET_DIR = XRAY_PATH_DEFAULT.parent / "rule-sets"

# Keys are the labels used by the common routing model. Values are paths in
# RuNetFreedom's sing-box.zip release archive.
SINGBOX_BINARY_RULE_SETS: dict[str, str] = {
    # Kept for custom/legacy rules created before regional presets.
    "geosite:category-ru": "rule-set-geosite/geosite-category-ru.srs",
    "geosite:ru-available-only-inside": "rule-set-geosite/geosite-ru-available-only-inside.srs",
    "geosite:ru-blocked": "rule-set-geosite/geosite-ru-blocked.srs",
    "geoip:ru": "rule-set-geoip/geoip-ru.srs",
    "geoip:ru-blocked": "rule-set-geoip/geoip-ru-blocked.srs",
    "geoip:ru-blocked-community": (
        "rule-set-geoip/geoip-ru-blocked-community.srs"
    ),
}

CHINA_SINGBOX_RULE_SETS: dict[str, str] = {
    key: f"https://raw.githubusercontent.com/2dust/sing-box-rules/{kind}/{tag}.srs"
    for key, kind, tag in (
        ("geosite:cn", "rule-set-geosite", "geosite-cn"),
        ("geosite:gfw", "rule-set-geosite", "geosite-gfw"),
        ("geosite:greatfire", "rule-set-geosite", "geosite-greatfire"),
        ("geosite:google", "rule-set-geosite", "geosite-google"),
        ("geoip:cn", "rule-set-geoip", "geoip-cn"),
        ("geoip:facebook", "rule-set-geoip", "geoip-facebook"),
        ("geoip:fastly", "rule-set-geoip", "geoip-fastly"),
        ("geoip:google", "rule-set-geoip", "geoip-google"),
        ("geoip:netflix", "rule-set-geoip", "geoip-netflix"),
        ("geoip:telegram", "rule-set-geoip", "geoip-telegram"),
        ("geoip:twitter", "rule-set-geoip", "geoip-twitter"),
    )
}

IRAN_SINGBOX_RULE_SETS: dict[str, str] = {
    key: f"https://raw.githubusercontent.com/chocolate4u/Iran-sing-box-rules/rule-set/{tag}.srs"
    for key, tag in (
        ("geosite:ir", "geosite-ir"),
        ("geoip:ir", "geoip-ir"),
    )
}

REGIONAL_SINGBOX_RULE_SETS: dict[str, dict[str, str]] = {
    "russia": {
        key: f"archive:{path}"
        for key, path in SINGBOX_BINARY_RULE_SETS.items()
    },
    "china": CHINA_SINGBOX_RULE_SETS,
    "iran": IRAN_SINGBOX_RULE_SETS,
}

ALL_SINGBOX_BINARY_RULE_SETS = {
    **SINGBOX_BINARY_RULE_SETS,
    **{key: f"{key.replace(':', '-')}.srs" for key in CHINA_SINGBOX_RULE_SETS},
    **{key: f"{key.replace(':', '-')}.srs" for key in IRAN_SINGBOX_RULE_SETS},
}


def singbox_rule_set_path(key: str) -> Path | None:
    archive_path = ALL_SINGBOX_BINARY_RULE_SETS.get(str(key).strip().lower())
    if not archive_path:
        return None
    return SINGBOX_RULE_SET_DIR / Path(archive_path).name
