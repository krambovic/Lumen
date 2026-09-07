"""Authenticated, cancellable loopback Clash API reads without ambient proxies."""
from __future__ import annotations

import http.client
import json
import socket
import threading
import time
from collections.abc import Mapping
from types import MappingProxyType
from typing import Any

MAX_DOCUMENT_BYTES = 4 * 1024 * 1024


def freeze_document(value: Any) -> Any:
    if isinstance(value, Mapping):
        return MappingProxyType({k: freeze_document(v) for k, v in value.items()})
    if isinstance(value, (list, tuple)):
        return tuple(freeze_document(v) for v in value)
    return value


class MetricsApiError(Exception):
    def __init__(self, reason: str, status: int = 0, *, unsupported: bool = False) -> None:
        super().__init__(reason)
        self.status = status
        self.unsupported = unsupported


class ClashApiClient:
    def __init__(self, port: int, secret: str) -> None:
        self.port = int(port)
        self._secret = str(secret or "")
        self._lock = threading.Lock()
        self._closed = False
        self._active: http.client.HTTPConnection | None = None

    def get(self, path: str, *, timeout: float = 1.0) -> Mapping[str, Any]:
        if not self._secret:
            raise MetricsApiError("Clash API authentication unavailable")
        if not 0 < self.port < 65536 or not path.startswith("/") or path.startswith("//"):
            raise MetricsApiError("Invalid loopback API target")
        deadline = time.monotonic() + max(0.05, min(3.0, timeout))
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout)
        with self._lock:
            if self._closed:
                raise MetricsApiError("stopped")
            self._active = connection
        try:
            # HTTPConnection ignores HTTP(S)_PROXY and NO_PROXY, and never follows
            # redirects. The bearer credential cannot escape this loopback peer.
            connection.request("GET", path, headers={"Authorization": f"Bearer {self._secret}"})
            with connection.getresponse() as response:
                if response.status != 200:
                    # Core delay handlers may report unsupported transports as
                    # 504. Inspect a bounded error body without logging it or
                    # treating capability errors as failed profile health.
                    body = response.read1(4096).decode("utf-8", errors="replace").lower()
                    unsupported = any(term in body for term in ("unsupported", "not supported", "not implemented"))
                    raise MetricsApiError(f"Clash API HTTP {response.status}", response.status, unsupported=unsupported)
                chunks, size = [], 0
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise MetricsApiError("Clash API deadline exceeded")
                    with self._lock:
                        if self._closed:
                            raise MetricsApiError("stopped")
                    if connection.sock is not None:
                        connection.sock.settimeout(remaining)
                    chunk = response.read1(min(65536, MAX_DOCUMENT_BYTES + 1 - size))
                    if not chunk:
                        break
                    chunks.append(chunk)
                    size += len(chunk)
                    if size > MAX_DOCUMENT_BYTES:
                        raise MetricsApiError("Clash API document exceeds limit")
            document = json.loads(b"".join(chunks))
            if not isinstance(document, dict):
                raise MetricsApiError("Invalid Clash API document")
            return freeze_document(document)
        except MetricsApiError:
            raise
        except Exception as exc:
            raise MetricsApiError(f"Clash API unavailable ({type(exc).__name__})") from None
        finally:
            connection.close()
            with self._lock:
                if self._active is connection:
                    self._active = None

    def close(self) -> None:
        with self._lock:
            self._closed = True
            connection = self._active
        if connection is not None:
            active_socket = connection.sock
            if active_socket is not None:
                try:
                    active_socket.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
            connection.close()
