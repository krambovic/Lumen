"""Verified HTTP GET latency on an established local proxy tunnel."""
from __future__ import annotations

import base64
from http.client import HTTPSConnection
import time
from urllib.parse import unquote, urlsplit

from .http_utils import abort_http_response, get_ssl_context


class WarmHTTPProbe:
    def __init__(self) -> None:
        self._connection: HTTPSConnection | None = None
        self._key: tuple[str, str] | None = None

    def close(self) -> None:
        connection, self._connection = self._connection, None
        self._key = None
        if connection is not None:
            abort_http_response(connection)

    def measure(self, proxy_url: str, url: str, timeout: float) -> int:
        proxy, target = urlsplit(proxy_url), urlsplit(url)
        if proxy.scheme != "http" or proxy.hostname not in {"127.0.0.1", "::1"}:
            raise ValueError("a guaranteed local HTTP proxy is required")
        if target.scheme != "https" or not target.hostname:
            raise ValueError("an HTTPS health target is required")
        key = (proxy_url, url)
        path = target.path or "/"
        if target.query:
            path += "?" + target.query
        try:
            connection = self._connection
            if key != self._key or connection is None or connection.sock is None:
                self.close()
                connection = HTTPSConnection(proxy.hostname, proxy.port or 80, timeout=timeout, context=get_ssl_context())
                headers = {}
                if proxy.username is not None:
                    credentials = unquote(proxy.username) + ":" + unquote(proxy.password or "")
                    headers["Proxy-Authorization"] = "Basic " + base64.b64encode(credentials.encode()).decode("ascii")
                connection.set_tunnel(target.hostname, target.port or 443, headers=headers)
                self._connection, self._key = connection, key
                connection.connect()
                self._request(connection, path)  # Discard the initial warm-up.
            samples = []
            for _ in range(2):
                if connection.sock is None:
                    raise OSError("proxy tunnel closed before measurement")
                started = time.perf_counter()
                self._request(connection, path)
                samples.append((time.perf_counter() - started) * 1000)
            return max(1, round(min(samples)))
        except Exception:
            self.close()
            raise

    @staticmethod
    def _request(connection: HTTPSConnection, path: str) -> None:
        if connection.sock is None:
            raise OSError("proxy tunnel unavailable")
        connection.request("GET", path, headers={"Connection": "keep-alive", "Cache-Control": "no-cache"})
        response = connection.getresponse()
        try:
            response.read(64 * 1024)
            if not 200 <= response.status < 300 or response.will_close or not response.isclosed():
                raise OSError("health target did not preserve the verified HTTP tunnel")
        finally:
            response.close()
