from __future__ import annotations

from dataclasses import dataclass
import json
from urllib.request import Request

from .constants import APP_VERSION
from .http_utils import urlopen


@dataclass(slots=True)
class UpdateInfo:
    version: str
    url: str
    notes: str = ""
    channel: str = "stable"
    digest_sha256: str = ""


def check_update(feed_url: str, channel: str = "stable", timeout: float = 5.0) -> UpdateInfo | None:
    if not feed_url:
        return None

    request = Request(feed_url, headers={"User-Agent": f"LumenKVN/{APP_VERSION}"})
    with urlopen(request, timeout=timeout) as response:
        payload = json.loads(response.read().decode("utf-8"))

    if isinstance(payload, dict) and "channels" in payload:
        release = payload.get("channels", {}).get(channel)
    else:
        release = payload

    if not isinstance(release, dict):
        return None
    version = str(release.get("version") or "")
    url = str(release.get("url") or "")
    if not version or not url:
        return None

    digest = str(release.get("digest") or release.get("sha256") or "")
    if digest.lower().startswith("sha256:"):
        digest = digest.split(":", 1)[1].strip()

    return UpdateInfo(
        version=version,
        url=url,
        notes=str(release.get("notes") or ""),
        channel=str(release.get("channel") or channel),
        digest_sha256=digest,
    )
