"""Persistent, loopback-only Xray StatsService client (no core subprocesses).

Wire contract verified against XTLS/Xray-core/app/stats/command/command.proto:
https://github.com/XTLS/Xray-core/blob/main/app/stats/command/command.proto
QueryStatsRequest: string pattern=1, bool reset=2 (omitted means false).
QueryStatsResponse: repeated Stat stat=1; Stat: string name=1, int64 value=2.
The small bounded codec below avoids a protobuf runtime/codegen dependency;
HTTP/2, gRPC framing, deadlines and cancellation are provided by grpcio.
"""
from __future__ import annotations

import importlib
import threading
import time
from collections.abc import Iterator

STATS_METHOD = "/xray.app.stats.command.StatsService/QueryStats"
MAX_RESPONSE_BYTES = 4 * 1024 * 1024


def _varint(value: int) -> bytes:
    result = bytearray()
    while value > 127:
        result.append((value & 127) | 128)
        value >>= 7
    result.append(value)
    return bytes(result)


def encode_query_stats_request(pattern: str = "inbound>>>") -> bytes:
    value = pattern.encode("utf-8")
    return b"\x0a" + _varint(len(value)) + value  # reset=false; never reset shared counters


def _read_varint(data: bytes, offset: int) -> tuple[int, int]:
    value = 0
    for shift in range(0, 70, 7):
        if offset >= len(data):
            raise ValueError("truncated protobuf varint")
        byte = data[offset]
        offset += 1
        if shift == 63 and byte > 1:
            raise ValueError("oversized protobuf varint")
        value |= (byte & 127) << shift
        if byte < 128:
            return value, offset
    raise ValueError("invalid protobuf varint")


def _fields(data: bytes) -> Iterator[tuple[int, int, bytes | int]]:
    offset = 0
    while offset < len(data):
        key, offset = _read_varint(data, offset)
        number, wire = key >> 3, key & 7
        if number == 0:
            raise ValueError("invalid protobuf field")
        if wire == 0:
            value, offset = _read_varint(data, offset)
        elif wire in (1, 2, 5):
            if wire == 2:
                size, offset = _read_varint(data, offset)
            else:
                size = 8 if wire == 1 else 4
            if size > len(data) - offset:
                raise ValueError("truncated protobuf field")
            value = data[offset:offset + size]
            offset += size
        else:
            raise ValueError("unsupported protobuf wire type")
        yield number, wire, value


def decode_query_stats_response(data: bytes) -> dict[str, int]:
    if len(data) > MAX_RESPONSE_BYTES:
        raise ValueError("stats response exceeds limit")
    stats: dict[str, int] = {}
    for number, wire, message in _fields(data):
        if number != 1:
            continue
        if wire != 2 or not isinstance(message, bytes):
            raise ValueError("invalid Stat message")
        name, value = "", 0
        for field, kind, raw in _fields(message):
            if field == 1:
                if kind != 2 or not isinstance(raw, bytes):
                    raise ValueError("invalid Stat name")
                name = raw.decode("utf-8", errors="strict")
            elif field == 2:
                if kind != 0 or not isinstance(raw, int):
                    raise ValueError("invalid Stat value")
                value = raw - (1 << 64) if raw >= (1 << 63) else raw
        if not name or value < 0 or name in stats:
            raise ValueError("invalid or duplicate traffic counter")
        stats[name] = value
    return stats


class XrayStatsClient:
    """Lazy reusable channel; failed samples are None with a diagnostic reason.

    ``close`` cancels an in-flight RPC without waiting for its deadline. Missing
    dependencies and transport errors never fall back to CLI or OS counters.
    """

    def __init__(self, port: int, *, timeout: float = 0.8, retry_interval: float = 3.0) -> None:
        self.port = int(port)
        self.timeout = max(0.05, min(2.0, float(timeout)))
        self.retry_interval = max(0.1, float(retry_interval))
        self.reason = "not sampled"
        self._lock = threading.Lock()
        self._channel = None
        self._rpc = None
        self._pending = None
        self._closed = False
        self._retry_at = 0.0

    def query(self) -> dict[str, int] | None:
        future = None
        try:
            with self._lock:
                if self._closed:
                    self.reason = "stopped"
                    return None
                if not 0 < self.port < 65536:
                    self.reason = "Xray stats API port unavailable"
                    return None
                if time.monotonic() < self._retry_at:
                    return None
                if self._channel is None:
                    try:
                        grpc = importlib.import_module("grpc")
                    except (ImportError, OSError):
                        self.reason = "grpcio unavailable; Xray metrics degraded"
                        self._retry_at = time.monotonic() + max(30.0, self.retry_interval)
                        return None
                    self._channel = grpc.insecure_channel(
                        f"127.0.0.1:{self.port}",
                        options=(("grpc.enable_http_proxy", 0),
                                 ("grpc.max_receive_message_length", MAX_RESPONSE_BYTES)),
                    )
                    self._rpc = self._channel.unary_unary(
                        STATS_METHOD,
                        request_serializer=lambda data: data,
                        response_deserializer=decode_query_stats_response,
                    )
                future = self._rpc.future(
                    encode_query_stats_request(), timeout=self.timeout, wait_for_ready=False,
                )
                self._pending = future
            result = future.result(timeout=self.timeout + 0.1)
            with self._lock:
                if self._closed:
                    self.reason = "stopped"
                    return None
                self.reason = ""
            return result
        except Exception as exc:
            if future is not None:
                future.cancel()
            with self._lock:
                self.reason = "stopped" if self._closed else f"Xray stats unavailable ({type(exc).__name__})"
                self._retry_at = time.monotonic() + self.retry_interval
                channel, self._channel = self._channel, None
                self._rpc = None
            if channel is not None:
                channel.close()
            return None
        finally:
            with self._lock:
                if self._pending is future:
                    self._pending = None

    def close(self) -> None:
        with self._lock:
            self._closed = True
            self.reason = "stopped"
            future, self._pending = self._pending, None
            channel, self._channel = self._channel, None
            self._rpc = None
        if future is not None:
            future.cancel()
        if channel is not None:
            channel.close()
