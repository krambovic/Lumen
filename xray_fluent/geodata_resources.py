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
    "geosite:category-ru": "rule-set-geosite/geosite-category-ru.srs",
    "geosite:ru-blocked": "rule-set-geosite/geosite-ru-blocked.srs",
    "geoip:ru": "rule-set-geoip/geoip-ru.srs",
    "geoip:ru-blocked": "rule-set-geoip/geoip-ru-blocked.srs",
    "geoip:ru-blocked-community": (
        "rule-set-geoip/geoip-ru-blocked-community.srs"
    ),
}


def singbox_rule_set_path(key: str) -> Path | None:
    archive_path = SINGBOX_BINARY_RULE_SETS.get(str(key).strip().lower())
    if not archive_path:
        return None
    return SINGBOX_RULE_SET_DIR / Path(archive_path).name
