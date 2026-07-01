"""Shared HTTP utilities with SSL error resilience."""

from __future__ import annotations

import ssl
import urllib.request
from urllib.request import Request


def _make_ssl_context() -> ssl.SSLContext:
    """Create a verified SSL context backed by the native Windows trust store.

    PyInstaller's embedded OpenSSL does not always see locally installed root
    certificates (for example antivirus HTTPS inspection certificates).
    ``truststore`` delegates validation to the operating system and keeps the
    same strict hostname and certificate checks as the standard context.
    """
    try:
        import truststore

        ctx = truststore.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    except (ImportError, RuntimeError):
        ctx = ssl.create_default_context()
    # Available since OpenSSL 3.0 / Python 3.10+
    if hasattr(ssl, "OP_IGNORE_UNEXPECTED_EOF"):
        ctx.options |= ssl.OP_IGNORE_UNEXPECTED_EOF
    return ctx


_ssl_ctx = _make_ssl_context()


def urlopen(request: Request | str, *, timeout: float = 15):
    """Drop-in replacement for urllib.request.urlopen with SSL fix."""
    return urllib.request.urlopen(request, timeout=timeout, context=_ssl_ctx)


def build_opener(*handlers: urllib.request.BaseHandler) -> urllib.request.OpenerDirector:
    """Build opener that uses the patched SSL context."""
    https_handler = urllib.request.HTTPSHandler(context=_ssl_ctx)
    return urllib.request.build_opener(https_handler, *handlers)


def build_proxy_opener(proxy_url: str | None = None) -> urllib.request.OpenerDirector:
    """Build opener with the app proxy when one is available."""
    if proxy_url:
        return build_opener(urllib.request.ProxyHandler({"http": proxy_url, "https": proxy_url}))
    return build_opener()


def urlopen_proxy_first(request: Request | str, *, timeout: float = 15, proxy_url: str | None = None):
    """Open through the local app proxy first, then fall back to direct."""
    if proxy_url:
        try:
            return build_proxy_opener(proxy_url).open(request, timeout=timeout)
        except Exception:
            pass
    return urlopen(request, timeout=timeout)
