"""GUI-free port of ``ui/node_edit_dialog.py`` outbound load/build logic"""
from __future__ import annotations

from copy import deepcopy
import json
import re
from typing import Any
from urllib.parse import quote

# Option lists — identical to ui/node_edit_dialog.py module constants.
FINGERPRINTS = ("", "chrome", "firefox", "safari", "ios", "android", "edge", "360", "qq", "random", "randomized")
NETWORKS = ("tcp", "ws", "grpc", "http", "h2", "xhttp", "httpupgrade", "kcp")
SECURITY = ("none", "tls", "reality")
FLOWS = ("", "xtls-rprx-vision", "xtls-rprx-vision-udp443")
RAW_HEADERS = ("none", "http")

XRAY_ADVANCED_PROTOCOLS = {"vless", "vmess"}
ENDPOINT_EDITABLE_PROTOCOLS = {
    "vless",
    "vmess",
    "trojan",
    "shadowsocks",
    "ss",
    "socks",
    "http",
    "hysteria",
    "hysteria2",
    "tuic",
    "mieru",
    "wireguard",
    "awg",
    "warp",
    "masque",
}
NATIVE_PROTOCOLS = {
    "wireguard",
    "awg",
    "warp",
    "hysteria",
    "hysteria2",
    "tuic",
    "mieru",
    "masque",
    "singbox_config",
}


def _first_dict(value: Any) -> dict[str, Any]:
    if isinstance(value, list) and value and isinstance(value[0], dict):
        return value[0]
    return {}


def _ensure_first_dict(parent: dict[str, Any], key: str) -> dict[str, Any]:
    value = parent.get(key)
    if not isinstance(value, list):
        value = []
        parent[key] = value
    if not value or not isinstance(value[0], dict):
        value.insert(0, {})
    return value[0]


def _set_or_remove(parent: dict[str, Any], key: str, value: str) -> None:
    if value:
        parent[key] = value
    else:
        parent.pop(key, None)


def _safe_port(value: Any, fallback: int) -> int:
    try:
        port = int(str(value).strip())
    except ValueError:
        return int(fallback or 0)
    return max(0, min(65535, port))


def _set_param(params: dict[str, str], key: str, value: Any) -> None:
    value = str(value or "")
    if value:
        params[key] = value


def _protocol_from_node(node) -> str:
    outbound = node.outbound if isinstance(getattr(node, "outbound", None), dict) else {}
    protocol = str(outbound.get("protocol") or node.scheme or "").strip().lower()
    return {
        "ss": "shadowsocks",
        "socks5": "socks",
        "hy": "hysteria",
        "hy2": "hysteria2",
        "wg": "wireguard",
    }.get(protocol, protocol)


def _field_capabilities(protocol: str) -> dict[str, bool | str]:
    protocol = (protocol or "").strip().lower()
    xray_advanced = protocol in XRAY_ADVANCED_PROTOCOLS
    tls_editable = protocol in {"vless", "vmess", "trojan"}
    endpoint = protocol in ENDPOINT_EDITABLE_PROTOCOLS
    fixed_endpoint = protocol in {"singbox_config", "xray_config"}
    return {
        "protocol": protocol,
        "xrayAdvanced": xray_advanced,
        "nativeSingbox": protocol in NATIVE_PROTOCOLS,
        "endpoint": endpoint and not fixed_endpoint,
        "identity": xray_advanced,
        "flow": protocol == "vless",
        "encryption": xray_advanced,
        "transport": xray_advanced,
        "rawHeader": xray_advanced,
        "tls": tls_editable,
        "reality": xray_advanced,
        "finalmask": xray_advanced,
        "basicOnly": not xray_advanced,
        "readOnlyConfig": protocol in {"singbox_config", "xray_config"},
    }


def _build_vless_link(name: str, server: str, port: int, outbound: dict[str, Any]) -> str:
    settings = outbound.get("settings") if isinstance(outbound.get("settings"), dict) else {}
    vnext = _first_dict(settings.get("vnext"))
    user = _first_dict(vnext.get("users"))
    user_id = str(user.get("id") or "")
    stream = outbound.get("streamSettings") if isinstance(outbound.get("streamSettings"), dict) else {}
    params: dict[str, str] = {
        "encryption": str(user.get("encryption") or "none"),
        "type": "tcp" if str(stream.get("network") or "tcp").lower() == "raw" else str(stream.get("network") or "tcp"),
    }
    _set_param(params, "flow", str(user.get("flow") or ""))
    security = str(stream.get("security") or "none").lower()
    _set_param(params, "security", security if security != "none" else "")
    tls = stream.get("tlsSettings") if isinstance(stream.get("tlsSettings"), dict) else {}
    reality = stream.get("realitySettings") if isinstance(stream.get("realitySettings"), dict) else {}
    payload = reality if security == "reality" else tls
    _set_param(params, "sni", str(payload.get("serverName") or ""))
    _set_param(params, "fp", str(payload.get("fingerprint") or ""))
    _set_param(params, "pcs", str(tls.get("pinnedPeerCertSha256") or ""))
    alpn = tls.get("alpn")
    _set_param(params, "alpn", ",".join(str(item) for item in alpn) if isinstance(alpn, list) else str(alpn or ""))
    if tls.get("allowInsecure") is True:
        _set_param(params, "allowInsecure", "1")
    network = str(stream.get("network") or "tcp").lower()
    if network == "ws":
        ws = stream.get("wsSettings") if isinstance(stream.get("wsSettings"), dict) else {}
        _set_param(params, "path", str(ws.get("path") or ""))
        headers = ws.get("headers") if isinstance(ws.get("headers"), dict) else {}
        _set_param(params, "host", str(headers.get("Host") or headers.get("host") or ""))
    elif network in {"http", "h2"}:
        http = stream.get("httpSettings") if isinstance(stream.get("httpSettings"), dict) else {}
        _set_param(params, "path", str(http.get("path") or ""))
        host = http.get("host")
        _set_param(params, "host", ",".join(str(item) for item in host) if isinstance(host, list) else str(host or ""))
    elif network == "grpc":
        grpc = stream.get("grpcSettings") if isinstance(stream.get("grpcSettings"), dict) else {}
        _set_param(params, "serviceName", str(grpc.get("serviceName") or ""))
    elif network == "xhttp":
        xhttp = stream.get("xhttpSettings") if isinstance(stream.get("xhttpSettings"), dict) else {}
        _set_param(params, "path", str(xhttp.get("path") or ""))
        _set_param(params, "host", str(xhttp.get("host") or ""))
        _set_param(params, "mode", str(xhttp.get("mode") or ""))
        extra = xhttp.get("extra")
        if extra not in (None, ""):
            _set_param(
                params,
                "extra",
                extra if isinstance(extra, str) else json.dumps(extra, ensure_ascii=False, separators=(",", ":")),
            )
    elif network == "httpupgrade":
        httpupgrade = stream.get("httpupgradeSettings") if isinstance(stream.get("httpupgradeSettings"), dict) else {}
        _set_param(params, "path", str(httpupgrade.get("path") or ""))
        _set_param(params, "host", str(httpupgrade.get("host") or ""))
    elif network in {"kcp", "mkcp"}:
        kcp = stream.get("kcpSettings") if isinstance(stream.get("kcpSettings"), dict) else {}
        header = kcp.get("header") if isinstance(kcp.get("header"), dict) else {}
        _set_param(params, "headerType", str(header.get("type") or "none"))
        for key in ("mtu", "tti", "uplinkCapacity", "downlinkCapacity", "readBufferSize", "writeBufferSize", "seed"):
            _set_param(params, key, kcp.get(key))
        if kcp.get("congestion") is True:
            _set_param(params, "congestion", "1")
    if security == "reality":
        _set_param(params, "pbk", str(reality.get("publicKey") or ""))
        _set_param(params, "sid", str(reality.get("shortId") or ""))
        _set_param(params, "spx", str(reality.get("spiderX") or ""))
        _set_param(params, "pqv", str(reality.get("mldsa65Verify") or ""))
    finalmask = stream.get("finalmask")
    if finalmask not in (None, ""):
        _set_param(params, "fm", finalmask if isinstance(finalmask, str) else json.dumps(finalmask, ensure_ascii=False, separators=(",", ":")))

    query = "&".join(f"{quote(key)}={quote(value, safe='')}" for key, value in params.items() if value != "")
    fragment = quote(name, safe="")
    return f"vless://{quote(user_id, safe='')}@{server}:{port}?{query}#{fragment}"


def _native_payload(outbound: dict[str, Any]) -> dict[str, Any]:
    native = outbound.get("singbox")
    return native if isinstance(native, dict) else {}


def _text_list(value: Any) -> str:
    if isinstance(value, (list, tuple)):
        return ", ".join(str(item) for item in value if str(item).strip())
    return str(value or "")


def _json_field(value: Any) -> str:
    if value in (None, ""):
        return ""
    if isinstance(value, str):
        return value
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def _editor_field(
    key: str,
    label: str,
    value: Any,
    *,
    kind: str = "text",
    options: tuple[str, ...] | list[str] = (),
    secret: bool = False,
    placeholder: str = "",
    when_key: str = "",
    when_values: tuple[str, ...] | list[str] = (),
) -> dict[str, Any]:
    result: dict[str, Any] = {
        "key": key,
        "label": label,
        "value": value if value is not None else "",
        "kind": kind,
    }
    if options:
        result["options"] = list(options)
    if secret:
        result["secret"] = True
    if placeholder:
        result["placeholder"] = placeholder
    if when_key:
        result["whenKey"] = when_key
        result["whenValues"] = list(when_values)
    return result


def _endpoint_fields(values: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        _editor_field("server", "Адрес", values.get("server", "")),
        _editor_field("port", "Порт", values.get("port", ""), kind="number"),
    ]


def _stream_editor_fields(values: dict[str, Any]) -> list[dict[str, Any]]:
    fields = [
        _editor_field("network", "Транспорт", values.get("network", "tcp"), kind="combo", options=NETWORKS),
        _editor_field(
            "transportPath",
            "Путь",
            values.get("transportPath", ""),
            when_key="network",
            when_values=("ws", "http", "h2", "xhttp", "httpupgrade"),
        ),
        _editor_field(
            "transportHost",
            "Host",
            values.get("transportHost", ""),
            when_key="network",
            when_values=("ws", "http", "h2", "xhttp", "httpupgrade"),
        ),
        _editor_field(
            "grpcServiceName",
            "gRPC service name",
            values.get("grpcServiceName", ""),
            when_key="network",
            when_values=("grpc",),
        ),
        _editor_field(
            "xhttpMode",
            "XHTTP mode",
            values.get("xhttpMode", "auto"),
            kind="combo",
            options=("auto", "packet-up", "stream-up", "stream-one"),
            when_key="network",
            when_values=("xhttp",),
        ),
        _editor_field(
            "xhttpExtra",
            "XHTTP extra (JSON)",
            values.get("xhttpExtra", ""),
            kind="area",
            when_key="network",
            when_values=("xhttp",),
        ),
        _editor_field(
            "kcpHeader",
            "mKCP header",
            values.get("kcpHeader", "none"),
            kind="combo",
            options=("none", "srtp", "utp", "wechat-video", "dtls", "wireguard"),
            when_key="network",
            when_values=("kcp",),
        ),
        _editor_field("kcpSeed", "mKCP seed", values.get("kcpSeed", ""), when_key="network", when_values=("kcp",)),
        _editor_field("kcpMtu", "mKCP MTU", values.get("kcpMtu", ""), kind="number", when_key="network", when_values=("kcp",)),
        _editor_field("kcpTti", "mKCP TTI", values.get("kcpTti", ""), kind="number", when_key="network", when_values=("kcp",)),
        _editor_field("kcpUplink", "mKCP upload", values.get("kcpUplink", ""), kind="number", when_key="network", when_values=("kcp",)),
        _editor_field("kcpDownlink", "mKCP download", values.get("kcpDownlink", ""), kind="number", when_key="network", when_values=("kcp",)),
        _editor_field("kcpCongestion", "mKCP congestion", values.get("kcpCongestion", False), kind="bool", when_key="network", when_values=("kcp",)),
    ]
    return fields


def _stream_security_fields(values: dict[str, Any]) -> list[dict[str, Any]]:
    secure = ("tls", "reality")
    return [
        _editor_field("security", "Защита", values.get("security", "none"), kind="combo", options=SECURITY),
        _editor_field("sni", "SNI", values.get("sni", ""), when_key="security", when_values=secure),
        _editor_field("fingerprint", "Fingerprint", values.get("fingerprint", ""), kind="combo", options=FINGERPRINTS, when_key="security", when_values=secure),
        _editor_field("allowInsecure", "Пропускать проверку сертификата", values.get("allowInsecure", False), kind="bool", when_key="security", when_values=("tls",)),
        _editor_field("alpn", "ALPN", values.get("alpn", ""), placeholder="h2, http/1.1", when_key="security", when_values=("tls",)),
        _editor_field("pinnedPeerCertSha256", "TLS SHA-256 pin", values.get("pinnedPeerCertSha256", ""), placeholder="64 hex-символа", when_key="security", when_values=("tls",)),
        _editor_field("publicKey", "REALITY public key", values.get("publicKey", ""), when_key="security", when_values=("reality",)),
        _editor_field("shortId", "REALITY ShortId", values.get("shortId", ""), when_key="security", when_values=("reality",)),
        _editor_field("spiderX", "REALITY SpiderX", values.get("spiderX", ""), when_key="security", when_values=("reality",)),
        _editor_field("pqv", "ML-DSA-65 verify", values.get("pqv", ""), when_key="security", when_values=("reality",)),
        _editor_field("finalmask", "Finalmask", values.get("finalmask", ""), kind="area"),
    ]


def _native_tls_fields(values: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        _editor_field("sni", "SNI", values.get("sni", "")),
        _editor_field("allowInsecure", "Пропускать проверку сертификата", values.get("allowInsecure", False), kind="bool"),
        _editor_field("alpn", "ALPN", values.get("alpn", ""), placeholder="h3, h2, http/1.1"),
    ]


_AMNEZIA_EDITOR_KEYS = (
    "jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4",
    "i1", "i2", "i3", "i4", "i5", "j1", "j2", "j3", "itime",
)


def _amnezia_editor_fields(values: dict[str, Any]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for key in _AMNEZIA_EDITOR_KEYS:
        kind = "number" if key in {"jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "itime"} else "area" if key.startswith(("i", "j")) and key not in {"jmin", "jmax"} else "text"
        result.append(_editor_field(f"awg_{key}", f"AWG {key.upper()}", values.get(f"awg_{key}", ""), kind=kind))
    return result


def _protocol_editor_fields(protocol: str, values: dict[str, Any], native: dict[str, Any]) -> list[dict[str, Any]]:
    if protocol in {"singbox_config", "xray_config"}:
        return []

    fields: list[dict[str, Any]] = []
    if protocol != "warp":
        fields.extend(_endpoint_fields(values))

    if protocol == "vless":
        fields.extend([
            _editor_field("uuid", "UUID / id", values.get("uuid", "")),
            _editor_field("encryption", "Шифрование", values.get("encryption", "none")),
            _editor_field("flow", "Flow", values.get("flow", ""), kind="combo", options=FLOWS),
        ])
        fields.extend(_stream_editor_fields(values))
        fields.extend(_stream_security_fields(values))
    elif protocol == "vmess":
        fields.extend([
            _editor_field("uuid", "UUID / id", values.get("uuid", "")),
            _editor_field("alterId", "Alter ID", values.get("alterId", 0), kind="number"),
            _editor_field("vmessSecurity", "Шифрование", values.get("vmessSecurity", "auto"), kind="combo", options=("auto", "none", "zero", "aes-128-gcm", "chacha20-poly1305")),
        ])
        fields.extend(_stream_editor_fields(values))
        fields.extend(_stream_security_fields(values))
    elif protocol == "trojan":
        fields.append(_editor_field("password", "Пароль", values.get("password", "")))
        fields.extend(_stream_editor_fields(values))
        fields.extend(_stream_security_fields(values))
    elif protocol == "shadowsocks":
        fields.extend([
            _editor_field("method", "Метод шифрования", values.get("method", "")),
            _editor_field("password", "Пароль", values.get("password", "")),
            _editor_field("plugin", "Плагин", values.get("plugin", "")),
        ])
    elif protocol in {"socks", "http"}:
        fields.extend([
            _editor_field("username", "Имя пользователя", values.get("username", "")),
            _editor_field("password", "Пароль", values.get("password", "")),
        ])
    elif protocol == "hysteria":
        fields.extend([
            _editor_field("auth", "Auth / пароль", values.get("auth", "")),
            _editor_field("hysteriaProtocol", "Протокол", values.get("hysteriaProtocol", "")),
            _editor_field("upMbps", "Скорость отдачи, Мбит/с", values.get("upMbps", ""), kind="number"),
            _editor_field("downMbps", "Скорость загрузки, Мбит/с", values.get("downMbps", ""), kind="number"),
            _editor_field("obfsPassword", "Obfs", values.get("obfsPassword", "")),
        ])
        fields.extend(_native_tls_fields(values))
    elif protocol == "hysteria2":
        fields.extend([
            _editor_field("password", "Пароль", values.get("password", "")),
            _editor_field("obfsType", "Obfs", values.get("obfsType", ""), kind="combo", options=("", "salamander")),
            _editor_field("obfsPassword", "Пароль obfs", values.get("obfsPassword", "")),
        ])
        fields.extend(_native_tls_fields(values))
    elif protocol == "tuic":
        fields.extend([
            _editor_field("uuid", "UUID", values.get("uuid", "")),
            _editor_field("password", "Пароль", values.get("password", "")),
            _editor_field("congestionControl", "Congestion control", values.get("congestionControl", "cubic"), kind="combo", options=("cubic", "bbr", "new_reno")),
            _editor_field("udpRelayMode", "UDP relay", values.get("udpRelayMode", "native"), kind="combo", options=("native", "quic")),
            _editor_field("zeroRtt", "0-RTT handshake", values.get("zeroRtt", False), kind="bool"),
        ])
        fields.extend(_native_tls_fields(values))
    elif protocol == "mieru":
        fields.extend([
            _editor_field("serverPorts", "Диапазоны портов", values.get("serverPorts", ""), placeholder="20000-30000, 40000"),
            _editor_field("username", "Имя пользователя", values.get("username", "")),
            _editor_field("password", "Пароль", values.get("password", "")),
            _editor_field("nativeTransport", "Транспорт", values.get("nativeTransport", "TCP"), kind="combo", options=("TCP",)),
            _editor_field("multiplexing", "Multiplexing", values.get("multiplexing", "")),
            _editor_field("trafficPattern", "Traffic pattern", values.get("trafficPattern", "")),
        ])
    elif protocol in {"wireguard", "awg"}:
        fields.extend([
            _editor_field("interfaceAddresses", "Адреса интерфейса", values.get("interfaceAddresses", ""), kind="area", placeholder="10.0.0.2/32, fd00::2/128"),
            _editor_field("privateKey", "Private key", values.get("privateKey", "")),
            _editor_field("wgPublicKey", "Peer public key", values.get("wgPublicKey", "")),
            _editor_field("preSharedKey", "Pre-shared key", values.get("preSharedKey", "")),
            _editor_field("allowedIps", "Allowed IPs", values.get("allowedIps", ""), kind="area"),
            _editor_field("mtu", "MTU", values.get("mtu", 1408), kind="number"),
            _editor_field("listenPort", "Локальный порт", values.get("listenPort", ""), kind="number"),
            _editor_field("keepalive", "Persistent keepalive", values.get("keepalive", ""), kind="number"),
            _editor_field("dns", "DNS профиля", values.get("dns", ""), kind="area"),
        ])
        if protocol == "awg" or isinstance(native.get("amnezia"), dict):
            fields.extend(_amnezia_editor_fields(values))
    elif protocol == "warp":
        fields.extend([
            _editor_field("profileId", "WARP profile ID", values.get("profileId", "")),
            _editor_field("profilePrivateKey", "WARP private key", values.get("profilePrivateKey", "")),
            _editor_field("authToken", "WARP auth token", values.get("authToken", "")),
            _editor_field("reserved", "Reserved bytes", values.get("reserved", ""), placeholder="1, 2, 3"),
            _editor_field("listenPort", "Локальный порт", values.get("listenPort", ""), kind="number"),
            _editor_field("keepalive", "Persistent keepalive", values.get("keepalive", ""), kind="number"),
            _editor_field("udpTimeout", "UDP timeout", values.get("udpTimeout", "5m0s")),
            _editor_field("dns", "DNS профиля", values.get("dns", ""), kind="area"),
        ])
        fields.extend(_amnezia_editor_fields(values))
    elif protocol == "masque":
        fields.extend([
            _editor_field("interfaceName", "Имя интерфейса", values.get("interfaceName", "masque0")),
            _editor_field("systemInterface", "Системный интерфейс", values.get("systemInterface", False), kind="bool"),
            _editor_field("useHttp2", "Использовать HTTP/2", values.get("useHttp2", False), kind="bool"),
            _editor_field("useIpv6", "Использовать IPv6", values.get("useIpv6", False), kind="bool"),
            _editor_field("interfaceAddresses", "Адреса интерфейса", values.get("interfaceAddresses", ""), kind="area"),
            _editor_field("privateKey", "Private key", values.get("privateKey", "")),
            _editor_field("wgPublicKey", "Server public key", values.get("wgPublicKey", "")),
            _editor_field("profileId", "Profile ID", values.get("profileId", "")),
            _editor_field("profilePrivateKey", "Profile private key", values.get("profilePrivateKey", "")),
            _editor_field("authToken", "Auth token", values.get("authToken", "")),
            _editor_field("allowedIps", "Allowed IPs", values.get("allowedIps", ""), kind="area"),
            _editor_field("mtu", "MTU", values.get("mtu", ""), kind="number"),
            _editor_field("udpTimeout", "UDP timeout", values.get("udpTimeout", "5m0s")),
            _editor_field("udpKeepalive", "UDP keepalive", values.get("udpKeepalive", "30s")),
            _editor_field("reconnectDelay", "Задержка переподключения", values.get("reconnectDelay", "5s")),
            _editor_field("congestionController", "Congestion controller", values.get("congestionController", "bbr")),
            _editor_field("sni", "SNI", values.get("sni", "")),
            _editor_field("allowInsecure", "Пропускать проверку сертификата", values.get("allowInsecure", False), kind="bool"),
            _editor_field("dns", "DNS профиля", values.get("dns", ""), kind="area"),
        ])
    return fields


def load_node_edit_fields(node) -> dict:
    """Flatten only fields supported by the selected node protocol."""
    outbound = deepcopy(node.outbound) if isinstance(node.outbound, dict) else {}
    protocol = _protocol_from_node(node)
    capabilities = _field_capabilities(protocol)
    native = _native_payload(outbound)
    fields: dict[str, Any] = {
        "name": node.name or "",
        "group": node.group or "",
        "server": node.server or "",
        "port": str(node.port or ""),
        "protocol": protocol.upper() or "?",
        "uuid": "",
        "encryption": "none",
        "flow": "",
        "alterId": 0,
        "vmessSecurity": "auto",
        "username": "",
        "password": "",
        "method": "",
        "plugin": "",
        "network": "tcp",
        "rawHeader": "none",
        "transportPath": "",
        "transportHost": "",
        "grpcServiceName": "",
        "xhttpMode": "auto",
        "xhttpExtra": "",
        "kcpHeader": "none",
        "kcpSeed": "",
        "kcpMtu": "",
        "kcpTti": "",
        "kcpUplink": "",
        "kcpDownlink": "",
        "kcpCongestion": False,
        "security": "none",
        "sni": "",
        "fingerprint": "",
        "allowInsecure": False,
        "alpn": "",
        "pinnedPeerCertSha256": "",
        "publicKey": "",
        "shortId": "",
        "spiderX": "",
        "pqv": "",
        "finalmask": "",
        "capabilities": capabilities,
        "editHint": _edit_hint(protocol, capabilities),
    }
    settings = outbound.get("settings") if isinstance(outbound.get("settings"), dict) else {}
    if protocol in {"vless", "vmess"}:
        vnext = _first_dict(settings.get("vnext"))
        user = _first_dict(vnext.get("users"))
        fields["uuid"] = str(user.get("id") or "")
        fields["encryption"] = str(user.get("encryption") or "none")
        fields["flow"] = str(user.get("flow") or "")
        fields["alterId"] = int(user.get("alterId") or 0)
        fields["vmessSecurity"] = str(user.get("security") or "auto")
    elif protocol in {"trojan", "shadowsocks", "socks", "http"}:
        server_item = _first_dict(settings.get("servers"))
        fields["password"] = str(server_item.get("password") or "")
        fields["method"] = str(server_item.get("method") or "")
        fields["plugin"] = str(server_item.get("plugin") or "")
        user = _first_dict(server_item.get("users"))
        fields["username"] = str(user.get("user") or "")
        if protocol in {"socks", "http"}:
            fields["password"] = str(user.get("pass") or "")

    stream = outbound.get("streamSettings") if isinstance(outbound.get("streamSettings"), dict) else {}
    fields["network"] = str(stream.get("network") or "tcp")
    raw = stream.get("rawSettings") if isinstance(stream.get("rawSettings"), dict) else {}
    header = raw.get("header") if isinstance(raw.get("header"), dict) else {}
    fields["rawHeader"] = str(header.get("type") or "none")
    network = str(stream.get("network") or "tcp").lower()
    if network == "ws":
        transport = stream.get("wsSettings") if isinstance(stream.get("wsSettings"), dict) else {}
        fields["transportPath"] = str(transport.get("path") or "")
        headers = transport.get("headers") if isinstance(transport.get("headers"), dict) else {}
        fields["transportHost"] = str(headers.get("Host") or headers.get("host") or "")
    elif network in {"http", "h2"}:
        transport = stream.get("httpSettings") if isinstance(stream.get("httpSettings"), dict) else stream.get("h2Settings") if isinstance(stream.get("h2Settings"), dict) else {}
        fields["transportPath"] = str(transport.get("path") or "")
        fields["transportHost"] = _text_list(transport.get("host"))
    elif network == "grpc":
        transport = stream.get("grpcSettings") if isinstance(stream.get("grpcSettings"), dict) else {}
        fields["grpcServiceName"] = str(transport.get("serviceName") or "")
    elif network == "xhttp":
        transport = stream.get("xhttpSettings") if isinstance(stream.get("xhttpSettings"), dict) else {}
        fields["transportPath"] = str(transport.get("path") or "")
        fields["transportHost"] = str(transport.get("host") or "")
        fields["xhttpMode"] = str(transport.get("mode") or "auto")
        fields["xhttpExtra"] = _json_field(transport.get("extra"))
    elif network == "httpupgrade":
        transport = stream.get("httpupgradeSettings") if isinstance(stream.get("httpupgradeSettings"), dict) else {}
        fields["transportPath"] = str(transport.get("path") or "")
        fields["transportHost"] = str(transport.get("host") or "")
    elif network in {"kcp", "mkcp"}:
        transport = stream.get("kcpSettings") if isinstance(stream.get("kcpSettings"), dict) else {}
        kcp_header = transport.get("header") if isinstance(transport.get("header"), dict) else {}
        fields["network"] = "kcp"
        fields["kcpHeader"] = str(kcp_header.get("type") or "none")
        fields["kcpSeed"] = str(transport.get("seed") or "")
        fields["kcpMtu"] = transport.get("mtu", "")
        fields["kcpTti"] = transport.get("tti", "")
        fields["kcpUplink"] = transport.get("uplinkCapacity", "")
        fields["kcpDownlink"] = transport.get("downlinkCapacity", "")
        fields["kcpCongestion"] = bool(transport.get("congestion", False))
    fields["security"] = str(stream.get("security") or "none")

    tls = stream.get("tlsSettings") if isinstance(stream.get("tlsSettings"), dict) else {}
    reality = stream.get("realitySettings") if isinstance(stream.get("realitySettings"), dict) else {}
    active_security = str(stream.get("security") or "").lower()
    security_payload = reality if active_security == "reality" else tls
    fields["sni"] = str(security_payload.get("serverName") or "")
    fields["fingerprint"] = str(security_payload.get("fingerprint") or "")
    fields["allowInsecure"] = bool(tls.get("allowInsecure", False))
    fields["alpn"] = _text_list(tls.get("alpn"))
    pinned = tls.get("pinnedPeerCertSha256")
    if isinstance(pinned, list):
        fields["pinnedPeerCertSha256"] = "~".join(str(item) for item in pinned if str(item))
    else:
        fields["pinnedPeerCertSha256"] = str(pinned or "")
    fields["publicKey"] = str(reality.get("publicKey") or "")
    fields["shortId"] = str(reality.get("shortId") or "")
    fields["spiderX"] = str(reality.get("spiderX") or "")
    fields["pqv"] = str(reality.get("mldsa65Verify") or "")
    finalmask = stream.get("finalmask")
    if finalmask not in (None, ""):
        fields["finalmask"] = (
            finalmask if isinstance(finalmask, str)
            else json.dumps(finalmask, ensure_ascii=False, separators=(",", ":"))
        )

    if native:
        tls_native = native.get("tls") if isinstance(native.get("tls"), dict) else {}
        fields["sni"] = str(tls_native.get("server_name") or fields["sni"])
        fields["allowInsecure"] = bool(tls_native.get("insecure", False))
        fields["alpn"] = _text_list(tls_native.get("alpn"))
        if protocol == "hysteria":
            fields["auth"] = str(native.get("auth_str") or "")
            fields["hysteriaProtocol"] = str(native.get("protocol") or "")
            fields["upMbps"] = native.get("up_mbps", native.get("up", ""))
            fields["downMbps"] = native.get("down_mbps", native.get("down", ""))
            fields["obfsPassword"] = str(native.get("obfs") or "")
        elif protocol == "hysteria2":
            fields["password"] = str(native.get("password") or "")
            obfs = native.get("obfs") if isinstance(native.get("obfs"), dict) else {}
            fields["obfsType"] = str(obfs.get("type") or "")
            fields["obfsPassword"] = str(obfs.get("password") or "")
        elif protocol == "tuic":
            fields["uuid"] = str(native.get("uuid") or "")
            fields["password"] = str(native.get("password") or "")
            fields["congestionControl"] = str(native.get("congestion_control") or "cubic")
            fields["udpRelayMode"] = str(native.get("udp_relay_mode") or "native")
            fields["zeroRtt"] = bool(native.get("zero_rtt_handshake", False))
        elif protocol == "mieru":
            fields["username"] = str(native.get("username") or "")
            fields["password"] = str(native.get("password") or "")
            fields["nativeTransport"] = str(native.get("transport") or "TCP").upper()
            fields["serverPorts"] = _text_list(native.get("server_ports"))
            fields["multiplexing"] = str(native.get("multiplexing") or "")
            fields["trafficPattern"] = str(native.get("traffic_pattern") or "")
        elif protocol in {"wireguard", "awg"}:
            peer = _first_dict(native.get("peers"))
            fields["server"] = str(peer.get("address") or peer.get("server") or fields["server"])
            fields["port"] = str(peer.get("port") or peer.get("server_port") or fields["port"])
            fields["interfaceAddresses"] = _text_list(native.get("address"))
            fields["privateKey"] = str(native.get("private_key") or "")
            fields["wgPublicKey"] = str(peer.get("public_key") or "")
            fields["preSharedKey"] = str(peer.get("pre_shared_key") or "")
            fields["allowedIps"] = _text_list(peer.get("allowed_ips"))
            fields["mtu"] = native.get("mtu", 1408)
            fields["listenPort"] = native.get("listen_port", "")
            fields["keepalive"] = peer.get("persistent_keepalive_interval", "")
            fields["dns"] = _text_list(outbound.get("_dns"))
        elif protocol == "warp":
            profile = native.get("profile") if isinstance(native.get("profile"), dict) else {}
            fields["profileId"] = str(profile.get("id") or "")
            fields["profilePrivateKey"] = str(profile.get("private_key") or "")
            fields["authToken"] = str(profile.get("auth_token") or "")
            fields["reserved"] = _text_list(native.get("reserved"))
            fields["listenPort"] = native.get("listen_port", "")
            fields["keepalive"] = native.get("persistent_keepalive_interval", "")
            fields["udpTimeout"] = str(native.get("udp_timeout") or "5m0s")
            fields["dns"] = _text_list(outbound.get("_dns"))
        elif protocol == "masque":
            profile = native.get("profile") if isinstance(native.get("profile"), dict) else {}
            fields["server"] = str(native.get("server") or fields["server"])
            fields["port"] = str(native.get("server_port") or fields["port"])
            fields["interfaceName"] = str(native.get("name") or "masque0")
            fields["systemInterface"] = bool(native.get("system", False))
            fields["useHttp2"] = bool(native.get("use_http2", False))
            fields["useIpv6"] = bool(native.get("use_ipv6", False))
            fields["interfaceAddresses"] = _text_list(native.get("address"))
            fields["privateKey"] = str(native.get("private_key") or "")
            fields["wgPublicKey"] = str(native.get("public_key") or "")
            fields["profileId"] = str(profile.get("id") or "")
            fields["profilePrivateKey"] = str(profile.get("private_key") or "")
            fields["authToken"] = str(profile.get("auth_token") or "")
            fields["allowedIps"] = _text_list(native.get("allowed_ips"))
            fields["mtu"] = native.get("mtu", "")
            fields["udpTimeout"] = str(native.get("udp_timeout") or "5m0s")
            fields["udpKeepalive"] = str(native.get("udp_keepalive_period") or "30s")
            fields["reconnectDelay"] = str(native.get("reconnect_delay") or "5s")
            fields["congestionController"] = str(native.get("congestion_controller") or "bbr")
            fields["dns"] = _text_list(outbound.get("_dns"))

        amnezia = native.get("amnezia") if isinstance(native.get("amnezia"), dict) else {}
        for key in _AMNEZIA_EDITOR_KEYS:
            fields[f"awg_{key}"] = amnezia.get(key, "")

    fields["protocolFields"] = _protocol_editor_fields(protocol, fields, native)
    return fields


def _edit_hint(protocol: str, capabilities: dict[str, bool | str]) -> str:
    return ""


def _split_editor_list(value: Any) -> list[str]:
    if isinstance(value, (list, tuple)):
        return [str(item).strip() for item in value if str(item).strip()]
    return [item.strip() for item in re.split(r"[,;\n]+", str(value or "")) if item.strip()]


def _editor_bool(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    return str(value or "").strip().lower() in {"1", "true", "yes", "on"}


def _editor_int(value: Any, fallback: int | None = None) -> int | None:
    if value in (None, ""):
        return fallback
    try:
        return int(str(value).strip())
    except (TypeError, ValueError) as exc:
        raise ValueError(f"Ожидалось целое число, получено: {value}") from exc


def _set_optional(parent: dict[str, Any], key: str, value: Any) -> None:
    if value not in (None, "", [], {}):
        parent[key] = value
    else:
        parent.pop(key, None)


def _update_stream_transport(stream: dict[str, Any], value) -> None:
    network = value("network", "tcp").lower() or "tcp"
    stream["network"] = network
    settings_keys = {
        "wsSettings", "httpSettings", "h2Settings", "grpcSettings",
        "xhttpSettings", "httpupgradeSettings", "kcpSettings", "rawSettings", "quicSettings",
    }
    active_key = {
        "ws": "wsSettings",
        "http": "httpSettings",
        "h2": "httpSettings",
        "grpc": "grpcSettings",
        "xhttp": "xhttpSettings",
        "httpupgrade": "httpupgradeSettings",
        "kcp": "kcpSettings",
    }.get(network)
    for key in settings_keys:
        if key != active_key:
            stream.pop(key, None)

    path = value("transportPath")
    host = value("transportHost")
    if network == "ws":
        settings = stream.setdefault("wsSettings", {})
        _set_optional(settings, "path", path)
        headers = settings.setdefault("headers", {})
        if host:
            headers["Host"] = host
        else:
            headers.pop("Host", None)
            headers.pop("host", None)
        if not headers:
            settings.pop("headers", None)
    elif network in {"http", "h2"}:
        settings = stream.setdefault("httpSettings", {})
        _set_optional(settings, "path", path)
        _set_optional(settings, "host", _split_editor_list(host))
    elif network == "grpc":
        settings = stream.setdefault("grpcSettings", {})
        _set_optional(settings, "serviceName", value("grpcServiceName"))
    elif network == "xhttp":
        settings = stream.setdefault("xhttpSettings", {})
        _set_optional(settings, "path", path)
        _set_optional(settings, "host", host)
        mode = value("xhttpMode", "auto") or "auto"
        settings["mode"] = mode
        extra_text = value("xhttpExtra")
        if extra_text:
            try:
                extra = json.loads(extra_text)
            except json.JSONDecodeError as exc:
                raise ValueError("XHTTP extra должен быть корректным JSON") from exc
            if not isinstance(extra, dict):
                raise ValueError("XHTTP extra должен быть JSON-объектом")
            settings["extra"] = extra
        else:
            settings.pop("extra", None)
    elif network == "httpupgrade":
        settings = stream.setdefault("httpupgradeSettings", {})
        _set_optional(settings, "path", path)
        _set_optional(settings, "host", host)
    elif network == "kcp":
        settings = stream.setdefault("kcpSettings", {})
        settings["header"] = {"type": value("kcpHeader", "none") or "none"}
        _set_optional(settings, "seed", value("kcpSeed"))
        for field_key, config_key in (
            ("kcpMtu", "mtu"),
            ("kcpTti", "tti"),
            ("kcpUplink", "uplinkCapacity"),
            ("kcpDownlink", "downlinkCapacity"),
        ):
            parsed = _editor_int(value(field_key), None)
            _set_optional(settings, config_key, parsed)
        settings["congestion"] = _editor_bool(value("kcpCongestion", False))


def _update_stream_security(stream: dict[str, Any], value) -> None:
    security = value("security", "none").lower() or "none"
    stream["security"] = security
    if security == "reality":
        stream.pop("tlsSettings", None)
        reality = stream.setdefault("realitySettings", {})
        _set_optional(reality, "serverName", value("sni"))
        _set_optional(reality, "fingerprint", value("fingerprint"))
        _set_optional(reality, "publicKey", value("publicKey"))
        _set_optional(reality, "shortId", value("shortId"))
        _set_optional(reality, "spiderX", value("spiderX"))
        _set_optional(reality, "mldsa65Verify", value("pqv"))
    elif security == "tls":
        stream.pop("realitySettings", None)
        tls = stream.setdefault("tlsSettings", {})
        _set_optional(tls, "serverName", value("sni"))
        _set_optional(tls, "fingerprint", value("fingerprint"))
        _set_optional(tls, "pinnedPeerCertSha256", value("pinnedPeerCertSha256").lower())
        tls["allowInsecure"] = _editor_bool(value("allowInsecure", False))
        _set_optional(tls, "alpn", _split_editor_list(value("alpn")))
    else:
        stream.pop("tlsSettings", None)
        stream.pop("realitySettings", None)

    finalmask = value("finalmask")
    if finalmask:
        try:
            stream["finalmask"] = json.loads(finalmask)
        except json.JSONDecodeError:
            stream["finalmask"] = finalmask
    else:
        stream.pop("finalmask", None)


def _update_native_tls(native: dict[str, Any], value) -> None:
    tls = native.setdefault("tls", {})
    tls["enabled"] = True
    _set_optional(tls, "server_name", value("sni"))
    tls["insecure"] = _editor_bool(value("allowInsecure", False))
    _set_optional(tls, "alpn", _split_editor_list(value("alpn")))


def _ensure_native(outbound: dict[str, Any], protocol: str) -> dict[str, Any]:
    native = outbound.get("singbox")
    if not isinstance(native, dict):
        native_type = "wireguard" if protocol == "awg" else protocol
        native = {"type": native_type, "tag": "proxy"}
        outbound["singbox"] = native
    return native


def build_node_updates(node, fields: dict) -> dict:
    """Rebuild a node using the field schema for its actual protocol."""
    current = load_node_edit_fields(node)

    def raw(key: str, default: Any = "") -> Any:
        if key in fields:
            return fields[key]
        return current.get(key, default)

    def g(key: str, default: str = "") -> str:
        return str(raw(key, default) or "").strip()

    outbound = deepcopy(node.outbound) if isinstance(node.outbound, dict) else {}
    protocol = _protocol_from_node(node)
    capabilities = _field_capabilities(protocol)
    server = g("server")
    port = _safe_port(raw("port", node.port), node.port)

    name = g("name", node.name or "")
    updates: dict[str, Any] = {
        "name": name,
        "group": g("group", node.group or "") or "Default",
    }

    if not capabilities.get("endpoint"):
        updates["outbound"] = outbound
        return updates

    if protocol != "warp":
        updates["server"] = server
        updates["port"] = port

    if protocol in {"vless", "vmess", "trojan", "shadowsocks", "socks", "http"}:
        settings = outbound.setdefault("settings", {})
        if protocol in {"vless", "vmess"}:
            vnext = _ensure_first_dict(settings, "vnext")
            vnext["address"] = server
            vnext["port"] = port
            user = _ensure_first_dict(vnext, "users")
            user["id"] = g("uuid")
            if protocol == "vless":
                user["encryption"] = g("encryption", "none") or "none"
                _set_or_remove(user, "flow", g("flow"))
            else:
                user["alterId"] = int(_editor_int(raw("alterId", 0), 0) or 0)
                user["security"] = g("vmessSecurity", "auto") or "auto"
        else:
            server_item = _ensure_first_dict(settings, "servers")
            server_item["address"] = server
            server_item["port"] = port
            if protocol == "trojan":
                server_item["password"] = g("password")
            elif protocol == "shadowsocks":
                server_item["method"] = g("method")
                server_item["password"] = g("password")
                _set_optional(server_item, "plugin", g("plugin"))
            else:
                username = g("username")
                if username:
                    server_item["users"] = [{"user": username, "pass": g("password")}]
                else:
                    server_item.pop("users", None)

        if protocol in {"vless", "vmess", "trojan"}:
            certificate_pin = g("pinnedPeerCertSha256").lower()
            if certificate_pin and not re.fullmatch(r"[0-9a-f]{64}", certificate_pin):
                raise ValueError("TLS SHA-256 pin должен содержать ровно 64 hex-символа")
            stream = outbound.setdefault("streamSettings", {})
            _update_stream_transport(stream, g)
            _update_stream_security(stream, g)
    else:
        native = _ensure_native(outbound, protocol)
        if protocol in {"hysteria", "hysteria2", "tuic", "mieru"}:
            native["server"] = server
            native["server_port"] = port
        if protocol == "hysteria":
            native["auth_str"] = g("auth")
            _set_optional(native, "protocol", g("hysteriaProtocol"))
            _set_optional(native, "up_mbps", _editor_int(raw("upMbps"), None))
            _set_optional(native, "down_mbps", _editor_int(raw("downMbps"), None))
            _set_optional(native, "obfs", g("obfsPassword"))
            _update_native_tls(native, g)
        elif protocol == "hysteria2":
            native["password"] = g("password")
            obfs_type = g("obfsType")
            obfs_password = g("obfsPassword")
            if obfs_type or obfs_password:
                native["obfs"] = {"type": obfs_type or "salamander"}
                _set_optional(native["obfs"], "password", obfs_password)
            else:
                native.pop("obfs", None)
            _update_native_tls(native, g)
        elif protocol == "tuic":
            native["uuid"] = g("uuid")
            _set_optional(native, "password", g("password"))
            native["congestion_control"] = g("congestionControl", "cubic") or "cubic"
            native["udp_relay_mode"] = g("udpRelayMode", "native") or "native"
            native["zero_rtt_handshake"] = _editor_bool(raw("zeroRtt", False))
            _update_native_tls(native, g)
        elif protocol == "mieru":
            native["username"] = g("username")
            native["password"] = g("password")
            native["transport"] = g("nativeTransport", "TCP").upper() or "TCP"
            _set_optional(native, "server_ports", _split_editor_list(raw("serverPorts")))
            _set_optional(native, "multiplexing", g("multiplexing"))
            _set_optional(native, "traffic_pattern", g("trafficPattern"))
        elif protocol in {"wireguard", "awg"}:
            peer = _ensure_first_dict(native, "peers")
            peer["address"] = server
            peer["port"] = port
            peer.pop("server", None)
            peer.pop("server_port", None)
            native["address"] = _split_editor_list(raw("interfaceAddresses"))
            native["private_key"] = g("privateKey")
            peer["public_key"] = g("wgPublicKey")
            _set_optional(peer, "pre_shared_key", g("preSharedKey"))
            peer["allowed_ips"] = _split_editor_list(raw("allowedIps"))
            native["mtu"] = int(_editor_int(raw("mtu", 1408), 1408) or 1408)
            _set_optional(native, "listen_port", _editor_int(raw("listenPort"), None))
            _set_optional(peer, "persistent_keepalive_interval", _editor_int(raw("keepalive"), None))
            _set_optional(outbound, "_dns", _split_editor_list(raw("dns")))
            if protocol == "awg":
                amnezia = native.setdefault("amnezia", {})
                for key in _AMNEZIA_EDITOR_KEYS:
                    raw_value = raw(f"awg_{key}", "")
                    if key in {"jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "itime"}:
                        parsed = _editor_int(raw_value, None)
                    else:
                        parsed = str(raw_value or "").strip()
                    _set_optional(amnezia, key, parsed)
                if not amnezia:
                    native.pop("amnezia", None)
        elif protocol == "warp":
            profile = native.setdefault("profile", {})
            profile["detour"] = "direct"
            _set_optional(profile, "id", g("profileId"))
            _set_optional(profile, "private_key", g("profilePrivateKey"))
            _set_optional(profile, "auth_token", g("authToken"))
            reserved = _split_editor_list(raw("reserved"))
            if reserved:
                try:
                    reserved_bytes = [int(item) for item in reserved]
                except ValueError as exc:
                    raise ValueError("Reserved должен содержать три числа от 0 до 255") from exc
                if len(reserved_bytes) != 3 or any(item < 0 or item > 255 for item in reserved_bytes):
                    raise ValueError("Reserved должен содержать три числа от 0 до 255")
                native["reserved"] = reserved_bytes
            else:
                native.pop("reserved", None)
            _set_optional(native, "listen_port", _editor_int(raw("listenPort"), None))
            _set_optional(native, "persistent_keepalive_interval", _editor_int(raw("keepalive"), None))
            _set_optional(native, "udp_timeout", g("udpTimeout"))
            _set_optional(outbound, "_dns", _split_editor_list(raw("dns")))
            amnezia = native.setdefault("amnezia", {})
            for key in _AMNEZIA_EDITOR_KEYS:
                raw_value = raw(f"awg_{key}", "")
                parsed = _editor_int(raw_value, None) if key in {"jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "itime"} else str(raw_value or "").strip()
                _set_optional(amnezia, key, parsed)
            if not amnezia:
                native.pop("amnezia", None)
        elif protocol == "masque":
            native["server"] = server
            native["server_port"] = port
            native["name"] = g("interfaceName", "masque0") or "masque0"
            native["system"] = _editor_bool(raw("systemInterface", False))
            native["use_http2"] = _editor_bool(raw("useHttp2", False))
            native["use_ipv6"] = _editor_bool(raw("useIpv6", False))
            _set_optional(native, "address", _split_editor_list(raw("interfaceAddresses")))
            _set_optional(native, "private_key", g("privateKey"))
            _set_optional(native, "public_key", g("wgPublicKey"))
            profile = native.setdefault("profile", {})
            profile["detour"] = "direct"
            _set_optional(profile, "id", g("profileId"))
            _set_optional(profile, "private_key", g("profilePrivateKey"))
            _set_optional(profile, "auth_token", g("authToken"))
            _set_optional(native, "allowed_ips", _split_editor_list(raw("allowedIps")))
            _set_optional(native, "mtu", _editor_int(raw("mtu"), None))
            _set_optional(native, "udp_timeout", g("udpTimeout"))
            _set_optional(native, "udp_keepalive_period", g("udpKeepalive"))
            _set_optional(native, "reconnect_delay", g("reconnectDelay"))
            _set_optional(native, "congestion_controller", g("congestionController"))
            tls = native.setdefault("tls", {})
            _set_optional(tls, "server_name", g("sni"))
            tls["insecure"] = _editor_bool(raw("allowInsecure", False))
            if not tls.get("server_name") and not tls.get("insecure"):
                native.pop("tls", None)
            _set_optional(outbound, "_dns", _split_editor_list(raw("dns")))

    link = (
        _build_vless_link(name, server, port, outbound)
        if protocol == "vless"
        else json.dumps(outbound, ensure_ascii=False, separators=(",", ":"))
    )
    updates["outbound"] = outbound
    updates["link"] = link
    return updates
