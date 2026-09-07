# Explicit proxy selection that cannot be bypassed by NO_PROXY.
import base64
from urllib.parse import unquote, urlsplit
from urllib.request import ProxyHandler
from .http_utils import build_opener
from .http_redirect_policy import SafeRedirectHandler


class StrictProxyHandler(ProxyHandler):
    def proxy_open(self, req, proxy, type):
        parsed = urlsplit(proxy)
        if parsed.scheme != "http" or not parsed.hostname:
            raise ValueError("Only an explicit HTTP proxy is supported")
        authority = parsed.hostname
        if ":" in authority:
            authority = "[" + authority + "]"
        authority += ":" + str(parsed.port or 80)
        if parsed.username is not None:
            credentials = unquote(parsed.username) + ":" + unquote(parsed.password or "")
            auth = base64.b64encode(credentials.encode()).decode("ascii")
            req.add_unredirected_header("Proxy-Authorization", "Basic " + auth)
        # Unlike urllib.ProxyHandler, deliberately do not consult proxy_bypass.
        req.set_proxy(authority, "http")
        return None


def proxy_opener(proxy_url: str):
    if not proxy_url:
        raise ValueError("Proxy URL is required; direct fallback is forbidden")
    return build_opener(StrictProxyHandler({"http": proxy_url, "https": proxy_url}), SafeRedirectHandler())
