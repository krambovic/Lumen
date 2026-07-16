from __future__ import annotations

from dataclasses import dataclass
import json
from urllib.request import Request

from .constants import APP_VERSION
from .http_utils import urlopen_proxy_first


@dataclass(slots=True)
class UpdateInfo:
    version: str
    url: str
    notes: str = ""
    channel: str = "stable"
    digest_sha256: str = ""


def check_update(feed_url: str, channel: str = "stable", timeout: float = 5.0, proxy_url: str | None = None) -> UpdateInfo | None:
    if not feed_url:
        return None

    attempts = (proxy_url, None) if proxy_url else (None,)
    for index, active_proxy in enumerate(attempts):
        request = Request(feed_url, headers={"User-Agent": f"LumenKVN/{APP_VERSION}"})
        try:
            with urlopen_proxy_first(request, timeout=timeout, proxy_url=active_proxy) as response:
                payload = json.loads(response.read().decode("utf-8"))
            break
        except Exception:
            if index == len(attempts) - 1:
                raise

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
