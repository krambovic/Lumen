from __future__ import annotations

from copy import deepcopy
from typing import Any

_SUPPORTED_NATIVE_PROTOCOLS = {
    "vless",
    "vmess",
    "trojan",
    "shadowsocks",
    "socks",
    "http",
    "warp",
    "wireguard",
    "awg",
}


def build_singbox_outbound(node, *, tag: str = "proxy") -> dict[str, Any]:
    """Convert a stored node outbound into a native sing-box outbound."""
    protocol = str((node.outbound or {}).get("protocol") or "").lower()
    if protocol not in _SUPPORTED_NATIVE_PROTOCOLS:
        raise ValueError(
            f"Текущий сервер нельзя конвертировать в native sing-box outbound: protocol `{protocol or 'unknown'}`"
        )

    outbound = _convert_outbound(deepcopy(node.outbound), tag=tag)
    unsupported_transport = str(outbound.pop("_unsupported_transport", "") or "").strip()
    if unsupported_transport:
        raise ValueError(
            f"Текущий сервер нельзя конвертировать в native sing-box outbound: transport `{unsupported_transport}` не поддерживается"
        )

    outbound["tag"] = tag
    return outbound


def _convert_outbound(xray_ob: dict[str, Any], *, tag: str = "proxy") -> dict[str, Any]:
    protocol = str(xray_ob.get("protocol") or "").lower()
    native = xray_ob.get("singbox")
    if isinstance(native, dict):
        sb = deepcopy(native)
        sb["tag"] = tag
        return sb

    xray_settings = dict(xray_ob.get("settings") or {})
    stream = dict(xray_ob.get("streamSettings") or {})

    sb: dict[str, Any] = {"type": protocol}

    if protocol in ("vless", "vmess"):
        vnext = (xray_settings.get("vnext") or [{}])[0]
        sb["server"] = str(vnext.get("address") or "")
        sb["server_port"] = int(vnext.get("port") or 0)
        users = (vnext.get("users") or [{}])[0]
        sb["uuid"] = str(users.get("id") or "")
        if protocol == "vless":
            flow = str(users.get("flow") or "")
            if flow:
                sb["flow"] = flow
        else:
            sb["alter_id"] = int(users.get("alterId") or 0)
            sb["security"] = str(users.get("security") or "auto")

    elif protocol == "trojan":
        servers = (xray_settings.get("servers") or [{}])[0]
        sb["server"] = str(servers.get("address") or "")
        sb["server_port"] = int(servers.get("port") or 0)
        sb["password"] = str(servers.get("password") or "")

    elif protocol == "shadowsocks":
        servers = (xray_settings.get("servers") or [{}])[0]
        sb["server"] = str(servers.get("address") or "")
        sb["server_port"] = int(servers.get("port") or 0)
        sb["method"] = str(servers.get("method") or "")
        sb["password"] = str(servers.get("password") or "")

    elif protocol in ("socks", "http"):
        servers = (xray_settings.get("servers") or [{}])[0]
        sb["server"] = str(servers.get("address") or "")
        sb["server_port"] = int(servers.get("port") or 0)
        user_list = servers.get("users") or []
        if user_list:
            sb["username"] = str(user_list[0].get("user") or "")
            sb["password"] = str(user_list[0].get("pass") or "")

    _apply_tls(sb, stream, str(sb.get("server") or ""))
    _apply_transport(sb, stream)
    return sb


def _apply_tls(sb: dict[str, Any], stream: dict[str, Any], server: str = "") -> None:
    security = str(stream.get("security") or "").lower()
    if security not in ("tls", "reality"):
        return

    tls: dict[str, Any] = {"enabled": True}

    if security == "reality":
        reality_settings = dict(stream.get("realitySettings") or {})
        server_name = str(reality_settings.get("serverName") or "").strip()
        if server_name:
            tls["server_name"] = server_name
        alpn = reality_settings.get("alpn")
        if alpn:
            tls["alpn"] = list(alpn) if isinstance(alpn, list) else [str(alpn)]
        fingerprint = str(reality_settings.get("fingerprint") or "")
        if fingerprint:
            tls["utls"] = {"enabled": True, "fingerprint": fingerprint}
        public_key = str(reality_settings.get("publicKey") or "")
        short_id = str(reality_settings.get("shortId") or "")
        tls["reality"] = {"enabled": True, "public_key": public_key, "short_id": short_id}
        tls["insecure"] = False
    else:
        tls_settings = dict(stream.get("tlsSettings") or {})
        server_name = str(tls_settings.get("serverName") or _infer_transport_host(stream) or server or "")
        if server_name:
            tls["server_name"] = server_name
        alpn = tls_settings.get("alpn")
        if alpn:
            tls["alpn"] = list(alpn) if isinstance(alpn, list) else [str(alpn)]
        fingerprint = str(tls_settings.get("fingerprint") or "")
        if fingerprint:
            tls["utls"] = {"enabled": True, "fingerprint": fingerprint}
        if tls_settings.get("allowInsecure", False):
            tls["insecure"] = True
        ech_config = str(tls_settings.get("echConfigList") or "")
        if ech_config:
            if "://" in ech_config:
                query_server, _, server_url = ech_config.partition("+")
                ech: dict[str, Any] = {"enabled": True}
                if query_server and server_url:
                    ech["query_server_name"] = query_server
                tls["ech"] = ech
            else:
                tls["ech"] = {
                    "enabled": True,
                    "config": [f"-----BEGIN ECH CONFIGS-----\n{ech_config}\n-----END ECH CONFIGS-----"],
                }

    sb["tls"] = tls


def _infer_transport_host(stream: dict[str, Any]) -> str:
    network = str(stream.get("network") or "").strip().lower()
    if network in {"tcp", "raw", "ws"}:
        settings = stream.get("wsSettings") if network == "ws" else stream.get("tcpSettings")
        if isinstance(settings, dict):
            headers = settings.get("headers") if network == "ws" else settings.get("header", {}).get("request", {}).get("headers")
            if isinstance(headers, dict):
                host = headers.get("Host") or headers.get("host")
                if isinstance(host, list):
                    return str(host[0]).strip() if host else ""
                return str(host or "").split(",")[0].strip()
    if network in {"http", "h2"}:
        settings = stream.get("httpSettings") if isinstance(stream.get("httpSettings"), dict) else {}
        host = settings.get("host")
        if isinstance(host, list) and host:
            return str(host[0]).strip()
        if isinstance(host, str):
            return host.split(",")[0].strip()
    if network == "grpc":
        settings = stream.get("grpcSettings") if isinstance(stream.get("grpcSettings"), dict) else {}
        return str(settings.get("authority") or "").strip()
    if network in {"xhttp", "httpupgrade"}:
        key = "xhttpSettings" if network == "xhttp" else "httpupgradeSettings"
        settings = stream.get(key) if isinstance(stream.get(key), dict) else {}
        return str(settings.get("host") or "").split(",")[0].strip()
    return ""


def _apply_transport(sb: dict[str, Any], stream: dict[str, Any]) -> None:
    network = str(stream.get("network") or "tcp").lower()
    if network == "tcp":
        return

    if network == "ws":
        ws_settings = dict(stream.get("wsSettings") or {})
        transport: dict[str, Any] = {"type": "ws"}
        path = str(ws_settings.get("path") or "")
        if path:
            transport["path"] = path
        headers = dict(ws_settings.get("headers") or {})
        if headers:
            transport["headers"] = headers
        sb["transport"] = transport
        return

    if network in ("http", "h2"):
        http_settings = dict(stream.get("httpSettings") or stream.get("h2Settings") or {})
        transport = {"type": "http"}
        host = http_settings.get("host")
        if host:
            transport["host"] = list(host) if isinstance(host, list) else [str(host)]
        path = str(http_settings.get("path") or "")
        if path:
            transport["path"] = path
        sb["transport"] = transport
        return

    if network == "grpc":
        grpc_settings = dict(stream.get("grpcSettings") or {})
        transport = {"type": "grpc"}
        service_name = str(grpc_settings.get("serviceName") or "")
        if service_name:
            transport["service_name"] = service_name
        sb["transport"] = transport
        return

    if network == "xhttp":
        xhttp_settings = dict(stream.get("xhttpSettings") or {})
        transport: dict[str, Any] = {"type": "xhttp"}
        mode = str(xhttp_settings.get("mode") or "").strip()
        if mode and mode != "auto":
            transport["mode"] = mode
        host = str(xhttp_settings.get("host") or "").strip()
        if host:
            transport["host"] = host
        path = str(xhttp_settings.get("path") or "").strip()
        if path:
            transport["path"] = path
        headers = xhttp_settings.get("headers")
        if isinstance(headers, dict) and headers:
            transport["headers"] = headers
        _copy_xhttp_value(xhttp_settings, transport, "scMaxEachPostBytes", "sc_max_each_post_bytes", int)
        _copy_xhttp_value(xhttp_settings, transport, "scMinPostsIntervalMs", "sc_min_posts_interval_ms", int)
        _copy_xhttp_value(xhttp_settings, transport, "xPaddingBytes", "x_padding_bytes", str)
        transport.setdefault("x_padding_bytes", "100-1000")
        _copy_xhttp_value(xhttp_settings, transport, "noGRPCHeader", "no_grpc_header", _to_bool)
        extra = xhttp_settings.get("extra")
        if isinstance(extra, dict):
            for key, value in extra.items():
                if isinstance(key, str) and key and key not in transport:
                    transport[key] = value
        sb["transport"] = transport
        return

    if network == "httpupgrade":
        settings = dict(stream.get("httpupgradeSettings") or {})
        transport = {"type": "httpupgrade"}
        path = str(settings.get("path") or "")
        if path:
            transport["path"] = path
        host = str(settings.get("host") or "")
        if host:
            transport["host"] = host
        headers = settings.get("headers")
        if isinstance(headers, dict) and headers:
            transport["headers"] = headers
        sb["transport"] = transport
        return

    if network in {"raw"}:
        sb["_unsupported_transport"] = network


def _copy_xhttp_value(
    source: dict[str, Any],
    target: dict[str, Any],
    source_key: str,
    target_key: str,
    caster,
) -> None:
    value = source.get(source_key)
    if value in (None, ""):
        return
    try:
        target[target_key] = caster(value)
    except Exception:
        target[target_key] = value


def _to_bool(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    return str(value).strip().lower() in {"1", "true", "yes", "on"}
