"""GUI-free port of ``ui/node_edit_dialog.py`` outbound load/build logic"""
from __future__ import annotations

from copy import deepcopy
import json
import re
from typing import Any
from urllib.parse import quote

# Option lists — identical to ui/node_edit_dialog.py module constants.
FINGERPRINTS = ("", "chrome", "firefox", "safari", "ios", "android", "edge", "360", "qq", "random", "randomized")
NETWORKS = ("tcp", "raw", "ws", "grpc", "http", "h2", "xhttp", "kcp", "quic")
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
    return str(outbound.get("protocol") or node.scheme or "").strip().lower()


def _field_capabilities(protocol: str) -> dict[str, bool | str]:
    protocol = (protocol or "").strip().lower()
    xray_advanced = protocol in XRAY_ADVANCED_PROTOCOLS
    tls_editable = protocol in {"vless", "vmess", "trojan"}
    endpoint = protocol in ENDPOINT_EDITABLE_PROTOCOLS
    fixed_endpoint = protocol in {"masque", "singbox_config"}
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
        "readOnlyConfig": protocol == "singbox_config",
    }


def _update_native_endpoint(outbound: dict[str, Any], server: str, port: int) -> None:
    native = outbound.get("singbox") if isinstance(outbound.get("singbox"), dict) else {}
    if not native:
        return
    protocol = str(native.get("type") or outbound.get("protocol") or "").strip().lower()
    if protocol in {"wireguard", "warp"}:
        peers = native.get("peers")
        peer = _first_dict(peers)
        if peer:
            if server:
                peer["server"] = server
            if port > 0:
                peer["server_port"] = port
        return
    if protocol in {"hysteria", "hysteria2", "tuic", "mieru"}:
        if server:
            native["server"] = server
        if port > 0:
            native["server_port"] = port


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
    network = str(stream.get("network") or "tcp").lower()
    if network == "xhttp":
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


def load_node_edit_fields(node) -> dict:
    """Flatten a node's editable fields for the QML form (mirrors
    ``NodeEditDialog._load_outbound_fields`` + the simple LineEdit fields)."""
    outbound = deepcopy(node.outbound) if isinstance(node.outbound, dict) else {}
    protocol = _protocol_from_node(node)
    capabilities = _field_capabilities(protocol)
    fields: dict[str, Any] = {
        "name": node.name or "",
        "group": node.group or "",
        "server": node.server or "",
        "port": str(node.port or ""),
        "protocol": protocol.upper() or "?",
        "uuid": "",
        "encryption": "none",
        "flow": "",
        "network": "tcp",
        "rawHeader": "none",
        "security": "none",
        "sni": "",
        "fingerprint": "",
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

    stream = outbound.get("streamSettings") if isinstance(outbound.get("streamSettings"), dict) else {}
    fields["network"] = str(stream.get("network") or "tcp")
    raw = stream.get("rawSettings") if isinstance(stream.get("rawSettings"), dict) else {}
    header = raw.get("header") if isinstance(raw.get("header"), dict) else {}
    fields["rawHeader"] = str(header.get("type") or "none")
    fields["security"] = str(stream.get("security") or "none")

    tls = stream.get("tlsSettings") if isinstance(stream.get("tlsSettings"), dict) else {}
    reality = stream.get("realitySettings") if isinstance(stream.get("realitySettings"), dict) else {}
    active_security = str(stream.get("security") or "").lower()
    security_payload = reality if active_security == "reality" else tls
    fields["sni"] = str(security_payload.get("serverName") or "")
    fields["fingerprint"] = str(security_payload.get("fingerprint") or "")
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
    return fields


def _edit_hint(protocol: str, capabilities: dict[str, bool | str]) -> str:
    return ""


def build_node_updates(node, fields: dict) -> dict:
    """Rebuild the outbound + scalar fields from the QML form values and return
    the ``updates`` dict for ``AppController.update_node`` (mirrors
    ``NodeEditDialog.get_updated_fields``)."""
    def g(key: str, default: str = "") -> str:
        return str(fields.get(key, default) or "").strip()

    certificate_pin = g("pinnedPeerCertSha256").lower()
    if certificate_pin and not re.fullmatch(r"[0-9a-f]{64}", certificate_pin):
        raise ValueError("TLS SHA-256 pin должен содержать ровно 64 hex-символа")

    outbound = deepcopy(node.outbound) if isinstance(node.outbound, dict) else {}
    protocol = _protocol_from_node(node)
    capabilities = _field_capabilities(protocol)
    server = g("server")
    port = _safe_port(fields.get("port", ""), node.port)

    name = g("name")
    updates: dict[str, Any] = {
        "name": name,
        "group": g("group") or "Default",
    }

    if not capabilities.get("endpoint"):
        updates["outbound"] = outbound
        return updates

    updates["server"] = server
    updates["port"] = port

    if not capabilities.get("xrayAdvanced"):
        _update_native_endpoint(outbound, server, port)
        if capabilities.get("tls"):
            stream = outbound.setdefault("streamSettings", {})
            if isinstance(stream, dict):
                security = g("security").lower() or "none"
                stream["security"] = security
                if security == "tls":
                    tls = stream.setdefault("tlsSettings", {})
                    if isinstance(tls, dict):
                        _set_or_remove(tls, "serverName", g("sni"))
                        _set_or_remove(tls, "fingerprint", g("fingerprint"))
                        _set_or_remove(
                            tls,
                            "pinnedPeerCertSha256",
                            certificate_pin,
                        )
                else:
                    stream.pop("tlsSettings", None)
        updates["outbound"] = outbound
        return updates

    settings = outbound.setdefault("settings", {})
    if isinstance(settings, dict) and protocol in {"vless", "vmess"}:
        vnext = _ensure_first_dict(settings, "vnext")
        vnext["address"] = server
        vnext["port"] = port
        user = _ensure_first_dict(vnext, "users")
        user["id"] = g("uuid")
        if protocol == "vless":
            user["encryption"] = g("encryption") or "none"
            _set_or_remove(user, "flow", g("flow"))

    stream = outbound.setdefault("streamSettings", {})
    if isinstance(stream, dict):
        network = g("network").lower() or "tcp"
        stream["network"] = network
        security = g("security").lower() or "none"
        stream["security"] = security
        if network == "raw":
            header_type = g("rawHeader") or "none"
            raw_settings = stream.setdefault("rawSettings", {})
            if isinstance(raw_settings, dict):
                header = raw_settings.setdefault("header", {})
                if isinstance(header, dict):
                    header["type"] = header_type
        elif "rawSettings" in stream:
            stream.pop("rawSettings", None)

        if security == "reality":
            stream.pop("tlsSettings", None)
            reality = stream.setdefault("realitySettings", {})
            if isinstance(reality, dict):
                _set_or_remove(reality, "serverName", g("sni"))
                _set_or_remove(reality, "fingerprint", g("fingerprint"))
                _set_or_remove(reality, "publicKey", g("publicKey"))
                _set_or_remove(reality, "shortId", g("shortId"))
                _set_or_remove(reality, "spiderX", g("spiderX"))
                _set_or_remove(reality, "mldsa65Verify", g("pqv"))
        elif security == "tls":
            stream.pop("realitySettings", None)
            tls = stream.setdefault("tlsSettings", {})
            if isinstance(tls, dict):
                _set_or_remove(tls, "serverName", g("sni"))
                _set_or_remove(tls, "fingerprint", g("fingerprint"))
                _set_or_remove(
                    tls,
                    "pinnedPeerCertSha256",
                    certificate_pin,
                )
        else:
            stream.pop("tlsSettings", None)
            stream.pop("realitySettings", None)

        finalmask = g("finalmask")
        if finalmask:
            try:
                stream["finalmask"] = json.loads(finalmask)
            except Exception:
                stream["finalmask"] = finalmask
        else:
            stream.pop("finalmask", None)

    link = _build_vless_link(name, server, port, outbound) if protocol == "vless" else node.link
    updates["outbound"] = outbound
    updates["link"] = link
    return updates
