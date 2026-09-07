"""HTTP health of the selected outbound, independent of TCP/UDP transport."""
from __future__ import annotations

import time
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlencode, urlsplit
from urllib.request import Request

from .metrics_api import ClashApiClient, MetricsApiError
from .traffic_route_classifier import RouteClassifier

HEALTH_MAX_AGE_SEC = 10.0
DEFAULT_HEALTH_URL = "https://www.gstatic.com/generate_204"


@dataclass(frozen=True, slots=True)
class HealthSample:
    status: str = "UNKNOWN"
    checked_at: float = 0.0  # time.monotonic(), never wall-clock or TCP ping time
    profile_id: str = ""
    latency_ms: int | None = None
    source: str = ""
    reason: str = "not sampled"

    def payload(self, now: float | None = None) -> dict[str, Any]:
        now = time.monotonic() if now is None else now
        fresh = self.checked_at > 0 and 0 <= now - self.checked_at <= HEALTH_MAX_AGE_SEC
        return {
            "health_status": self.status if fresh else "UNKNOWN",
            "health_checked_at": self.checked_at,
            "health_profile_id": self.profile_id,
            "health_source": self.source,
            "health_reason": self.reason if fresh else "health sample stale or unavailable",
            "latency_ms": self.latency_ms if fresh and self.status == "HEALTHY" else None,
        }


def probe_active_profile(
    *, profile_id: str, clash_client: ClashApiClient | None = None,
    outbound_tag: str = "proxy", outbound_graph: Mapping[str, Any] | None = None,
    proxy_url: str = "", health_url: str = DEFAULT_HEALTH_URL, timeout: float = 1.2,
) -> HealthSample:
    """A proxy_url may be supplied ONLY for a guaranteed active-outbound listener.

    A listening server/TCP handshake is not health. Missing credentials, a
    failed local API, unsupported protocols, stale data or unverified routing
    are UNKNOWN, not failed. Local API errors are not remote-profile failures.
    """
    started = time.monotonic()
    source = "clash-active-outbound" if clash_client is not None else "active-http-proxy"

    def sample(status: str, reason: str = "", latency: int | None = None) -> HealthSample:
        return HealthSample(status, time.monotonic(), profile_id, latency, source, reason)

    if not profile_id:
        return sample("UNKNOWN", "active profile identity unavailable")
    try:
        target = urlsplit(health_url)
        parsed = urlsplit(proxy_url)
    except ValueError:
        return sample("UNKNOWN", "invalid HTTP health target")
    if target.scheme not in {"https", "http"} or not target.hostname:
        return sample("UNKNOWN", "unsupported HTTP health URL")
    if clash_client is not None:
        if RouteClassifier(outbound_graph).classify([outbound_tag]) != "proxy":
            return sample("UNKNOWN", "active outbound type/selection unverified")
        path = "/proxies/" + quote(outbound_tag, safe="") + "/delay?" + urlencode({
            "url": health_url, "timeout": max(100, int(timeout * 1000) - 200),
        })
        try:
            result = clash_client.get(path, timeout=timeout)
        except MetricsApiError as exc:
            if exc.unsupported:
                return sample("UNKNOWN", "active outbound HTTP probe unsupported")
            # sing-box getProxyDelay uses 503 for an explicit test error and
            # 504 for its context deadline, not generic local API failures.
            if exc.status in {408, 503, 504}:
                return sample("FAILED", "active outbound HTTP probe failed")
            return sample("UNKNOWN", str(exc))
        delay = result.get("delay")
        if isinstance(delay, bool) or not isinstance(delay, int) or delay <= 0:
            return sample("UNKNOWN", "active outbound delay unsupported or invalid")
        return sample("HEALTHY", latency=delay)

    if parsed.scheme != "http" or parsed.hostname not in {"127.0.0.1", "::1"}:
        return sample("UNKNOWN", "guaranteed active local HTTP proxy unavailable")
    try:
        # StrictProxyHandler deliberately ignores NO_PROXY, including '*'.
        # Do not replace this with urllib.urlopen or a best-effort proxy handler.
        from .strict_proxy import proxy_opener
        opener = proxy_opener(proxy_url)
        request = Request(health_url, headers={"Cache-Control": "no-cache"}, method="GET")
        with opener.open(request, timeout=timeout) as response:
            status = response.status
            if 200 <= status < 300:
                return sample("HEALTHY", latency=max(0, round((time.monotonic() - started) * 1000)))
            return sample("FAILED", f"active profile HTTP {status}")
    except ImportError:
        return sample("UNKNOWN", "strict proxy probing unavailable")
    except HTTPError as exc:
        if exc.code in {405, 407, 501}:
            return sample("UNKNOWN", f"active proxy probe unsupported (HTTP {exc.code})")
        return sample("FAILED", f"active profile HTTP {exc.code}")
    except (TimeoutError, URLError, OSError):
        return sample("FAILED", "active profile HTTP request failed")
    except Exception:
        return sample("UNKNOWN", "active profile probe unavailable")
