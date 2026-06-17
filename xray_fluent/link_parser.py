from __future__ import annotations

import base64
import ipaddress
import json
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, unquote, urlsplit
from urllib.request import url2pathname

from .models import Node


class LinkParseError(ValueError):
    pass


def parse_links_text(text: str) -> tuple[list[Node], list[str]]:
    stripped = text.strip()
    file_text = _read_import_file_reference(stripped)
    if file_text is not None:
        stripped = file_text.strip()
        text = file_text
    if stripped.startswith(("{", "[")):
        try:
            return _parse_json_nodes_text(stripped)
        except Exception as exc:
            if stripped.startswith("{"):
                return [], [f"JSON: {exc}"]
    lowered = stripped.lower()
    if "[interface]" in lowered and "[peer]" in lowered:
        try:
            return [_parse_wireguard_config(stripped)], []
        except Exception as exc:
            return [], [f"Config: {exc}"]

    lines = [line.strip() for line in text.splitlines() if line.strip()]
    nodes: list[Node] = []
    errors: list[str] = []

    for idx, line in enumerate(lines, start=1):
        try:
            node = parse_single(line)
            nodes.append(node)
        except Exception as exc:
            errors.append(f"Line {idx}: {exc}")

    return nodes, errors


def _read_import_file_reference(text: str) -> str | None:
    """Accept paths copied from Explorer/QML instead of treating them as links."""
    if not text or "\n" in text:
        return None
    candidate_text = text.strip().strip('"')
    parsed = urlsplit(candidate_text)
    path_text = ""
    if len(candidate_text) >= 3 and candidate_text[1] == ":" and candidate_text[2] in {"\\", "/"}:
        path_text = candidate_text
    elif parsed.scheme.lower() == "file":
        path_text = url2pathname(unquote(parsed.path or ""))
        if parsed.netloc and not path_text.startswith("\\\\"):
            path_text = f"\\\\{parsed.netloc}{path_text}"
    elif parsed.scheme == "":
        candidate = candidate_text
        if candidate.lower().endswith((".conf", ".txt", ".json")):
            path_text = candidate
    if not path_text:
        return None
    path = Path(path_text)
    if not path.is_file():
        return None
    return path.read_text(encoding="utf-8", errors="replace")


def parse_single(raw: str) -> Node:
    text = raw.strip()
    if not text:
        raise LinkParseError("empty input")

    if text.startswith("{"):
        return _parse_json_outbound(text)
    if text.startswith("["):
        try:
            nodes, errors = _parse_json_nodes_text(text)
        except Exception:
            nodes, errors = [], []
        if nodes:
            return nodes[0]
        if errors:
            raise LinkParseError("; ".join(errors[:3]))
    if text.startswith("["):
        return _parse_wireguard_config(text)

    scheme = urlsplit(text).scheme.lower()
    if scheme == "vless":
        return _parse_vless(text)
    if scheme == "vmess":
        return _parse_vmess(text)
    if scheme == "trojan":
        return _parse_trojan(text)
    if scheme == "ss":
        return _parse_shadowsocks(text)
    if scheme in {"socks", "socks5"}:
        return _parse_socks(text)
    if scheme in {"http", "https"}:
        return _parse_http_proxy(text)
    if scheme in {"wireguard", "wg", "awg", "warp"}:
        return _parse_wireguard_like_link(text, scheme)
    if scheme in {"hysteria", "hy"}:
        return _parse_hysteria(text)
    if scheme in {"hysteria2", "hy2"}:
        return _parse_hysteria2(text)

    if scheme == "tuic":
        return _parse_tuic(text)

    raise LinkParseError(f"unsupported scheme: {scheme or 'unknown'}")


def _first(query: dict[str, list[str]], key: str, default: str = "") -> str:
    values = query.get(key)
    if not values:
        return default
    return values[0]


def _get_param(params: dict[str, str], *keys: str, default: str = "") -> str:
    empty_value: str | None = None
    for key in keys:
        if key in params:
            value = params[key]
            if value:
                return value
            if empty_value is None:
                empty_value = value

    lower_params = {str(key).lower(): value for key, value in params.items()}
    for key in keys:
        lowered = str(key).lower()
        if lowered in lower_params:
            value = lower_params[lowered]
            if value:
                return value
            if empty_value is None:
                empty_value = value
    if empty_value is not None:
        return empty_value
    return default


def _decode_b64(data: str) -> str:
    data = data.strip()
    data += "=" * ((4 - len(data) % 4) % 4)
    try:
        raw = base64.urlsafe_b64decode(data.encode("utf-8"))
    except Exception:
        raw = base64.b64decode(data.encode("utf-8"))
    return raw.decode("utf-8")


def _clean_name(name: str, fallback: str) -> str:
    value = unquote(name).strip()
    return value if value else fallback


def _json_name(payload: dict[str, Any], fallback: str) -> str:
    for key in ("remarks", "remark", "ps", "name", "tag"):
        value = str(payload.get(key) or "").strip()
        if value:
            return value
    return fallback


def _to_bool(value: str) -> bool:
    return str(value).lower() in {"1", "true", "yes", "on"}


def _build_stream_settings(params: dict[str, str], default_network: str = "tcp", default_security: str = "none") -> dict[str, Any]:
    network = (_get_param(params, "type", "net", default=default_network or "tcp") or "tcp").lower()
    security = (_get_param(params, "security", "tls", default=default_security or "none") or "none").lower()
    if security == "none" and _get_param(params, "tls") == "tls":
        security = "tls"

    stream: dict[str, Any] = {
        "network": network,
        "security": security,
    }

    host = _get_param(params, "host")
    path = _get_param(params, "path")

    if network == "ws":
        ws_settings: dict[str, Any] = {}
        if path:
            ws_settings["path"] = path
        if host:
            ws_settings["headers"] = {"Host": host}
        stream["wsSettings"] = ws_settings
    elif network in {"http", "h2"}:
        http_settings: dict[str, Any] = {}
        if host:
            http_settings["host"] = [h.strip() for h in host.split(",") if h.strip()]
        if path:
            http_settings["path"] = path
        stream["httpSettings"] = http_settings
    elif network == "grpc":
        grpc_settings: dict[str, Any] = {}
        service_name = _get_param(params, "serviceName", "service_name")
        if service_name:
            grpc_settings["serviceName"] = service_name
        authority = _get_param(params, "authority")
        if authority:
            grpc_settings["authority"] = authority
        mode = _get_param(params, "mode")
        if mode == "multi":
            grpc_settings["multiMode"] = True
        stream["grpcSettings"] = grpc_settings
    elif network == "xhttp":
        xhttp_settings: dict[str, Any] = {}
        if path:
            xhttp_settings["path"] = path
        if host:
            xhttp_settings["host"] = host
        mode = _get_param(params, "mode")
        if mode in {"auto", "packet-up", "stream-up", "stream-one"}:
            xhttp_settings["mode"] = mode
        extra = _get_param(params, "extra")
        if extra:
            try:
                decoded_extra = json.loads(extra)
            except Exception:
                decoded_extra = None
            if isinstance(decoded_extra, dict):
                xhttp_settings["extra"] = decoded_extra
        for key in (
            "scMaxEachPostBytes",
            "scMaxBufferedPosts",
            "scMinPostsIntervalMs",
            "xPaddingBytes",
            "padding",
            "downloadSettings",
            "download_settings",
            "noGRPCHeader",
        ):
            value = _get_param(params, key)
            if value:
                if key in {"downloadSettings", "download_settings"}:
                    try:
                        xhttp_settings[key] = json.loads(unquote(value))
                    except Exception:
                        xhttp_settings[key] = value
                else:
                    xhttp_settings[key] = value
        stream["xhttpSettings"] = xhttp_settings
    elif network == "httpupgrade":
        httpupgrade_settings: dict[str, Any] = {}
        if path:
            httpupgrade_settings["path"] = path
        if host:
            httpupgrade_settings["host"] = host
        early_data = _get_param(params, "ed")
        if early_data:
            httpupgrade_settings["ed"] = early_data
        stream["httpupgradeSettings"] = httpupgrade_settings
    elif network == "quic":
        stream["quicSettings"] = {
            "security": _get_param(params, "quicSecurity", "quic_security") or "none",
            "key": _get_param(params, "key") or "",
            "header": {"type": _get_param(params, "headerType", "header_type") or "none"},
        }
    elif network == "kcp":
        stream["kcpSettings"] = {
            "header": {"type": _get_param(params, "headerType", "header_type") or "none"},
        }

    if security == "tls":
        tls_settings: dict[str, Any] = {}
        sni = _get_param(params, "sni", "serverName", "server_name")
        if sni:
            tls_settings["serverName"] = sni
        alpn = _get_param(params, "alpn")
        if alpn:
            tls_settings["alpn"] = [item.strip() for item in alpn.split(",") if item.strip()]
        fp = _get_param(params, "fp", "fingerprint")
        if fp:
            tls_settings["fingerprint"] = fp
        allow_insecure = _get_param(params, "allowInsecure", "allow_insecure", "insecure")
        if allow_insecure:
            tls_settings["allowInsecure"] = _to_bool(allow_insecure)
        cert_sha = _get_param(params, "pcs", "pinnedPeerCertSha256", "certSha")
        if cert_sha:
            tls_settings["pinnedPeerCertSha256"] = cert_sha
        ech = _get_param(params, "ech", "echConfigList")
        if ech:
            tls_settings["echConfigList"] = ech
            tls_settings["echForceQuery"] = "full"
        verify_names = _get_param(params, "vcn", "verifyPeerCertByName")
        if verify_names:
            tls_settings["verifyPeerCertByName"] = [item.strip() for item in verify_names.split(",") if item.strip()]
        stream["tlsSettings"] = tls_settings
    elif security == "reality":
        reality_settings: dict[str, Any] = {}
        sni = _get_param(params, "sni", "serverName", "server_name")
        if sni:
            reality_settings["serverName"] = sni
        fp = _get_param(params, "fp", "fingerprint")
        if fp:
            reality_settings["fingerprint"] = fp
        pbk = _get_param(params, "pbk", "publicKey", "public_key", "password")
        if pbk:
            reality_settings["publicKey"] = pbk
        sid = _get_param(params, "sid", "shortId", "short_id")
        if sid:
            reality_settings["shortId"] = sid
        spx = _get_param(params, "spx", "spiderX", "spider_x")
        if spx:
            reality_settings["spiderX"] = spx
        pqv = _get_param(params, "pqv", "mldsa65Verify", "mldsa65_verify")
        if pqv:
            reality_settings["mldsa65Verify"] = pqv
        reality_settings["show"] = False
        stream["realitySettings"] = reality_settings

    finalmask = _get_param(params, "fm", "finalmask")
    if finalmask:
        try:
            stream["finalmask"] = json.loads(finalmask)
        except Exception:
            stream["finalmask"] = finalmask

    return stream


def _parse_vless(link: str) -> Node:
    parsed = urlsplit(link)
    query = {k: v for k, v in parse_qs(parsed.query, keep_blank_values=True).items()}
    params = {k: _first(query, k) for k in query}

    user_id = unquote(parsed.username or "")
    server = parsed.hostname or ""
    port = parsed.port or 443

    if not user_id or not server:
        raise LinkParseError("invalid vless link")

    user: dict[str, Any] = {
        "id": user_id,
        "encryption": _get_param(params, "encryption") or "none",
    }
    flow = _get_param(params, "flow")
    if flow:
        user["flow"] = flow

    stream_settings = _build_stream_settings(params, default_network="tcp", default_security=params.get("security", "none"))
    outbound = {
        "protocol": "vless",
        "settings": {
            "vnext": [
                {
                    "address": server,
                    "port": port,
                    "users": [user],
                }
            ]
        },
        "streamSettings": stream_settings,
    }
    if str(stream_settings.get("network") or "").lower() == "xhttp":
        outbound["mux"] = {"enabled": False, "concurrency": -1}

    name = _clean_name(parsed.fragment, f"vless-{server}:{port}")
    return Node(
        name=name,
        scheme="vless",
        server=server,
        port=port,
        link=link,
        outbound=outbound,
    )


def repair_node_outbound_from_link(node: Node) -> bool:
    link = str(node.link or "").strip()
    if not link:
        return False
    try:
        reparsed = parse_single(link)
    except Exception:
        return False
    if reparsed.outbound == node.outbound:
        return False
    node.outbound = reparsed.outbound
    if not node.scheme:
        node.scheme = reparsed.scheme
    if not node.server:
        node.server = reparsed.server
    if node.port <= 0:
        node.port = reparsed.port
    return True


def normalize_node_outbound(node: Node | None) -> bool:
    if node is None or not isinstance(node.outbound, dict):
        return False
    return normalize_outbound_for_runtime(node.outbound, node.server)


def normalize_outbound_for_runtime(outbound: dict[str, Any], server: str = "") -> bool:
    changed = False
    stream_settings = outbound.get("streamSettings")
    if not isinstance(stream_settings, dict):
        return False

    if str(stream_settings.get("network") or "").strip().lower() == "xhttp":
        mux = outbound.get("mux")
        if not isinstance(mux, dict):
            outbound["mux"] = {"enabled": False, "concurrency": -1}
            changed = True
        else:
            if mux.get("enabled") is not False:
                mux["enabled"] = False
                changed = True
            if mux.get("concurrency") != -1:
                mux["concurrency"] = -1
                changed = True

    security = str(stream_settings.get("security") or "").strip().lower()
    if security == "tls":
        tls_settings = stream_settings.get("tlsSettings")
        if not isinstance(tls_settings, dict):
            tls_settings = {}
            stream_settings["tlsSettings"] = tls_settings
            changed = True
        if not str(tls_settings.get("serverName") or "").strip():
            inferred = _infer_transport_host(stream_settings) or server
            if inferred:
                tls_settings["serverName"] = inferred
                changed = True
        if tls_settings.get("echConfigList") and not tls_settings.get("echForceQuery"):
            tls_settings["echForceQuery"] = "full"
            changed = True
    elif security == "reality":
        reality_settings = stream_settings.get("realitySettings")
        if not isinstance(reality_settings, dict):
            reality_settings = {}
            stream_settings["realitySettings"] = reality_settings
            changed = True
        if reality_settings.get("show") is not False:
            reality_settings["show"] = False
            changed = True
    return changed


def _infer_transport_host(stream_settings: dict[str, Any]) -> str:
    network = str(stream_settings.get("network") or "").strip().lower()
    if network == "ws":
        headers = (stream_settings.get("wsSettings") or {}).get("headers") if isinstance(stream_settings.get("wsSettings"), dict) else {}
        if isinstance(headers, dict):
            return str(headers.get("Host") or headers.get("host") or "").split(",")[0].strip()
    if network in {"http", "h2"}:
        http_settings = stream_settings.get("httpSettings")
        if isinstance(http_settings, dict):
            hosts = http_settings.get("host")
            if isinstance(hosts, list) and hosts:
                return str(hosts[0]).strip()
            if isinstance(hosts, str):
                return hosts.split(",")[0].strip()
    if network == "grpc":
        grpc_settings = stream_settings.get("grpcSettings")
        if isinstance(grpc_settings, dict):
            return str(grpc_settings.get("authority") or "").strip()
    return ""


def validate_node_outbound(node: Node) -> str | None:
    outbound = node.outbound if isinstance(node.outbound, dict) else {}
    stream_settings = outbound.get("streamSettings") if isinstance(outbound, dict) else None
    if not isinstance(stream_settings, dict):
        return None

    security = str(stream_settings.get("security") or "").strip().lower()
    if security != "reality":
        return None

    reality_settings = stream_settings.get("realitySettings")
    if not isinstance(reality_settings, dict):
        reality_settings = {}

    public_key = str(reality_settings.get("publicKey") or "").strip()
    node_name = str(node.name or node.server or "\u0431\u0435\u0437\u044b\u043c\u044f\u043d\u043d\u044b\u0439 \u0441\u0435\u0440\u0432\u0435\u0440").strip()
    if not public_key:
        return (
            f"\u0421\u0435\u0440\u0432\u0435\u0440 {node_name} \u043d\u0435 \u043c\u043e\u0436\u0435\u0442 \u0431\u044b\u0442\u044c \u0437\u0430\u043f\u0443\u0449\u0435\u043d: \u0434\u043b\u044f REALITY \u043e\u0431\u044f\u0437\u0430\u0442\u0435\u043b\u0435\u043d publicKey "
            "(\u043f\u0430\u0440\u0430\u043c\u0435\u0442\u0440 pbk \u0432 VLESS-\u0441\u0441\u044b\u043b\u043a\u0435), \u043d\u043e \u0432 \u044d\u0442\u043e\u0439 \u0441\u0441\u044b\u043b\u043a\u0435 \u043e\u043d \u043f\u0443\u0441\u0442\u043e\u0439 \u0438\u043b\u0438 \u043e\u0442\u0441\u0443\u0442\u0441\u0442\u0432\u0443\u0435\u0442."
        )

    short_id = str(reality_settings.get("shortId") or "").strip()
    if short_id and (len(short_id) > 16 or len(short_id) % 2 != 0 or any(ch not in "0123456789abcdefABCDEF" for ch in short_id)):
        return (
            f"\u0421\u0435\u0440\u0432\u0435\u0440 {node_name} \u043d\u0435 \u043c\u043e\u0436\u0435\u0442 \u0431\u044b\u0442\u044c \u0437\u0430\u043f\u0443\u0449\u0435\u043d: shortId \u0434\u043b\u044f REALITY \u0434\u043e\u043b\u0436\u0435\u043d \u0431\u044b\u0442\u044c hex-\u0441\u0442\u0440\u043e\u043a\u043e\u0439 "
            "\u0447\u0435\u0442\u043d\u043e\u0439 \u0434\u043b\u0438\u043d\u044b \u0434\u043e 16 \u0441\u0438\u043c\u0432\u043e\u043b\u043e\u0432. \u041f\u0440\u043e\u0432\u0435\u0440\u044c\u0442\u0435 \u043f\u0430\u0440\u0430\u043c\u0435\u0442\u0440 sid \u0432 \u0441\u0441\u044b\u043b\u043a\u0435."
        )
    return None


def _parse_vmess(link: str) -> Node:
    encoded = link[len("vmess://") :]
    payload = json.loads(_decode_b64(encoded))

    server = str(payload.get("add") or "")
    port = int(payload.get("port") or 443)
    user_id = str(payload.get("id") or "")
    if not server or not user_id:
        raise LinkParseError("invalid vmess link")

    security = str(payload.get("tls") or "none").lower()
    params = {
        "net": str(payload.get("net") or "tcp"),
        "type": str(payload.get("net") or "tcp"),
        "security": "tls" if security in {"tls", "reality"} else "none",
        "host": str(payload.get("host") or ""),
        "path": str(payload.get("path") or ""),
        "sni": str(payload.get("sni") or payload.get("host") or ""),
        "alpn": str(payload.get("alpn") or ""),
        "fp": str(payload.get("fp") or ""),
        "serviceName": str(payload.get("serviceName") or ""),
    }

    outbound = {
        "protocol": "vmess",
        "settings": {
            "vnext": [
                {
                    "address": server,
                    "port": port,
                    "users": [
                        {
                            "id": user_id,
                            "alterId": int(payload.get("aid") or 0),
                            "security": str(payload.get("scy") or "auto"),
                        }
                    ],
                }
            ]
        },
        "streamSettings": _build_stream_settings(params, default_network=params["net"], default_security=params["security"]),
    }

    name = _clean_name(str(payload.get("ps") or ""), f"vmess-{server}:{port}")
    return Node(
        name=name,
        scheme="vmess",
        server=server,
        port=port,
        link=link,
        outbound=outbound,
    )


def _parse_trojan(link: str) -> Node:
    parsed = urlsplit(link)
    query = parse_qs(parsed.query, keep_blank_values=True)
    params = {k: _first(query, k) for k in query}

    password = unquote(parsed.username or "")
    server = parsed.hostname or ""
    port = parsed.port or 443
    if not password or not server:
        raise LinkParseError("invalid trojan link")

    outbound = {
        "protocol": "trojan",
        "settings": {
            "servers": [
                {
                    "address": server,
                    "port": port,
                    "password": password,
                }
            ]
        },
        "streamSettings": _build_stream_settings(params, default_network="tcp", default_security=params.get("security", "tls")),
    }

    name = _clean_name(parsed.fragment, f"trojan-{server}:{port}")
    return Node(
        name=name,
        scheme="trojan",
        server=server,
        port=port,
        link=link,
        outbound=outbound,
    )


def _parse_shadowsocks(link: str) -> Node:
    parsed = urlsplit(link)
    query = parse_qs(parsed.query, keep_blank_values=True)

    method = ""
    password = ""
    server = parsed.hostname or ""
    port = parsed.port or 8388

    if parsed.username and parsed.password:
        method = unquote(parsed.username)
        password = unquote(parsed.password)
    elif parsed.username and not parsed.password:
        decoded = _decode_b64(parsed.username)
        if ":" not in decoded:
            raise LinkParseError("invalid shadowsocks credentials")
        method, password = decoded.split(":", 1)
    else:
        decoded = _decode_b64(parsed.netloc)
        parsed_decoded = urlsplit(f"ss://{decoded}")
        if parsed_decoded.username and parsed_decoded.password and parsed_decoded.hostname:
            method = unquote(parsed_decoded.username)
            password = unquote(parsed_decoded.password)
            server = parsed_decoded.hostname
            port = parsed_decoded.port or 8388
        else:
            raise LinkParseError("invalid shadowsocks link")

    if not method or not password or not server:
        raise LinkParseError("invalid shadowsocks link")

    plugin = _first(query, "plugin")
    outbound_server: dict[str, Any] = {
        "address": server,
        "port": port,
        "method": method,
        "password": password,
    }
    if plugin:
        outbound_server["plugin"] = plugin

    outbound = {
        "protocol": "shadowsocks",
        "settings": {
            "servers": [outbound_server],
        },
    }

    name = _clean_name(parsed.fragment, f"ss-{server}:{port}")
    return Node(
        name=name,
        scheme="ss",
        server=server,
        port=port,
        link=link,
        outbound=outbound,
    )


def _parse_socks(link: str) -> Node:
    parsed = urlsplit(link)
    server = parsed.hostname or ""
    port = parsed.port or 1080
    if not server:
        raise LinkParseError("invalid socks link")

    user = unquote(parsed.username or "")
    password = unquote(parsed.password or "")

    server_item: dict[str, Any] = {
        "address": server,
        "port": port,
    }
    if user:
        server_item["users"] = [{"user": user, "pass": password}]

    outbound = {
        "protocol": "socks",
        "settings": {"servers": [server_item]},
    }

    name = _clean_name(parsed.fragment, f"socks-{server}:{port}")
    return Node(
        name=name,
        scheme="socks",
        server=server,
        port=port,
        link=link,
        outbound=outbound,
    )


def _parse_http_proxy(link: str) -> Node:
    parsed = urlsplit(link)
    server = parsed.hostname or ""
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    if not server:
        raise LinkParseError("invalid http proxy link")

    user = unquote(parsed.username or "")
    password = unquote(parsed.password or "")

    server_item: dict[str, Any] = {
        "address": server,
        "port": port,
    }
    if user:
        server_item["users"] = [{"user": user, "pass": password}]

    outbound = {
        "protocol": "http",
        "settings": {"servers": [server_item]},
    }

    name = _clean_name(parsed.fragment, f"http-{server}:{port}")
    return Node(
        name=name,
        scheme="http",
        server=server,
        port=port,
        link=link,
        outbound=outbound,
    )


def _parse_json_nodes_text(text: str) -> tuple[list[Node], list[str]]:
    payload = json.loads(text)
    return _parse_json_nodes_payload(payload)


def _parse_json_nodes_payload(payload: Any) -> tuple[list[Node], list[str]]:
    nodes: list[Node] = []
    errors: list[str] = []

    if isinstance(payload, list):
        items = payload
    elif isinstance(payload, dict):
        links = payload.get("links")
        if isinstance(links, list):
            items = links
        elif isinstance(payload.get("configs"), list):
            items = payload["configs"]
        elif isinstance(payload.get("nodes"), list):
            items = payload["nodes"]
        elif isinstance(payload.get("items"), list):
            items = payload["items"]
        elif _json_payload_can_be_node(payload):
            items = [payload]
        else:
            raise LinkParseError("JSON must contain links, configs, nodes, items, protocol, type, endpoints, or outbounds")
    else:
        raise LinkParseError("JSON subscription must be an object or an array")

    for idx, item in enumerate(items, start=1):
        try:
            if isinstance(item, str):
                nodes.append(parse_single(item))
            elif isinstance(item, dict):
                nodes.append(_parse_json_outbound_payload(item))
            else:
                raise LinkParseError(f"unsupported JSON item type: {type(item).__name__}")
        except Exception as exc:
            errors.append(f"JSON item {idx}: {exc}")

    return nodes, errors


def _json_payload_can_be_node(payload: dict[str, Any]) -> bool:
    return (
        "type" in payload
        or "protocol" in payload
        or isinstance(payload.get("endpoints"), list)
        or isinstance(payload.get("outbounds"), list)
    )


def _parse_json_outbound(text: str) -> Node:
    payload = json.loads(text)
    if not isinstance(payload, dict):
        raise LinkParseError("JSON node must be an object")
    return _parse_json_outbound_payload(payload)


def _parse_json_outbound_payload(payload: dict[str, Any]) -> Node:
    original_payload = payload

    outbound: dict[str, Any]
    if "type" in payload:
        outbound = _native_singbox_outbound(payload)
    elif "protocol" in payload:
        outbound = dict(payload)
    elif isinstance(payload.get("endpoints"), list) and payload["endpoints"]:
        outbound = _native_singbox_outbound(payload["endpoints"][0])
    elif isinstance(payload.get("outbounds"), list) and payload["outbounds"]:
        first_outbound = _pick_json_proxy_outbound(payload["outbounds"])
        outbound = _native_singbox_outbound(first_outbound) if "type" in first_outbound else first_outbound
    else:
        raise LinkParseError("JSON must contain `protocol` or `outbounds`")

    protocol = str(outbound.get("protocol") or "custom")
    tag = str(outbound.get("tag") or protocol)
    server = ""
    port = 0

    native = outbound.get("singbox") if isinstance(outbound.get("singbox"), dict) else {}
    settings = outbound.get("settings") or {}
    if protocol in {"vless", "vmess"}:
        if native:
            server = str(native.get("server") or "")
            port = int(native.get("server_port") or 0)
        else:
            vnext = (settings.get("vnext") or [{}])[0]
            server = str(vnext.get("address") or "")
            port = int(vnext.get("port") or 0)
    elif protocol in {"trojan", "shadowsocks", "socks", "http"}:
        if native:
            server = str(native.get("server") or "")
            port = int(native.get("server_port") or 0)
        else:
            servers = (settings.get("servers") or [{}])[0]
            server = str(servers.get("address") or "")
            port = int(servers.get("port") or 0)
    elif protocol in {"warp", "wireguard", "awg"} and native:
        peers = native.get("peers")
        peer = peers[0] if isinstance(peers, list) and peers and isinstance(peers[0], dict) else {}
        server = str(peer.get("address") or native.get("server") or "")
        port = int(peer.get("port") or native.get("server_port") or 0)
    elif protocol in {"hysteria", "hysteria2", "tuic"} and native:
        server = str(native.get("server") or "")
        port = int(native.get("server_port") or 0)

    return Node(
        name=_json_name(original_payload, f"json-{tag}"),
        scheme=protocol,
        server=server,
        port=port,
        link=json.dumps(original_payload, ensure_ascii=False, separators=(",", ":")),
        outbound=outbound,
    )


def _pick_json_proxy_outbound(outbounds: list[Any]) -> dict[str, Any]:
    candidates = [dict(item) for item in outbounds if isinstance(item, dict)]
    if not candidates:
        raise LinkParseError("JSON outbounds list is empty")

    def kind(item: dict[str, Any]) -> str:
        return str(item.get("protocol") or item.get("type") or "").strip().lower()

    ignored = {"freedom", "blackhole", "dns", "direct", "block", "selector", "urltest", "url-test"}
    supported = {
        "vless",
        "vmess",
        "trojan",
        "shadowsocks",
        "socks",
        "http",
        "warp",
        "wireguard",
        "awg",
        "hysteria",
        "hysteria2",
        "hy",
        "hy2",
        "tuic",
    }
    for item in candidates:
        if str(item.get("tag") or "").strip().lower() == "proxy" and kind(item) not in ignored:
            return item
    for item in candidates:
        if kind(item) in supported:
            return item
    for item in candidates:
        if kind(item) not in ignored:
            return item
    return candidates[0]


def _native_singbox_outbound(payload: dict[str, Any]) -> dict[str, Any]:
    native = dict(payload)
    protocol = str(native.get("type") or "custom").lower()
    if protocol == "hy":
        protocol = native["type"] = "hysteria"
    elif protocol == "hy2":
        protocol = native["type"] = "hysteria2"
    return {
        "protocol": "awg" if protocol == "wireguard" and isinstance(native.get("amnezia"), dict) else protocol,
        "singbox": native,
    }


def _parse_hysteria(link: str) -> Node:
    parsed = urlsplit(link)
    query = parse_qs(parsed.query, keep_blank_values=True)
    params = {k: _first(query, k) for k in query}
    server = parsed.hostname or _get_param(params, "server", "address", "host")
    port = parsed.port or int(_get_param(params, "port", default="0") or 0) or 443
    auth = unquote(parsed.username or "") or _get_param(params, "auth", "auth_str", "authStr", "password")
    if not server or not auth:
        raise LinkParseError("hysteria link must contain server and auth/password")

    outbound: dict[str, Any] = {
        "type": "hysteria",
        "tag": "proxy",
        "server": server,
        "server_port": int(port),
    }
    if auth:
        outbound["auth_str"] = auth
    protocol = _get_param(params, "protocol")
    if protocol:
        outbound["protocol"] = protocol
    up_mbps = _get_param(params, "upmbps", "up_mbps", "up")
    down_mbps = _get_param(params, "downmbps", "down_mbps", "down")
    if up_mbps:
        outbound["up_mbps"] = int(up_mbps)
    if down_mbps:
        outbound["down_mbps"] = int(down_mbps)
    _apply_hysteria_tls(outbound, params, server)
    _apply_hysteria_obfs(outbound, params)
    name = _clean_name(parsed.fragment, f"hysteria-{server}:{port}")
    return Node(name=name, scheme="hysteria", server=server, port=int(port), link=link, outbound=_native_singbox_outbound(outbound))


def _parse_hysteria2(link: str) -> Node:
    parsed = urlsplit(link)
    query = parse_qs(parsed.query, keep_blank_values=True)
    params = {k: _first(query, k) for k in query}
    server = parsed.hostname or _get_param(params, "server", "address", "host")
    port = parsed.port or int(_get_param(params, "port", default="0") or 0) or 443
    password = unquote(parsed.username or "") or _get_param(params, "password", "auth", "auth_str", "authStr")
    if not server or not password:
        raise LinkParseError("hysteria2 link must contain server and password")

    outbound: dict[str, Any] = {
        "type": "hysteria2",
        "tag": "proxy",
        "server": server,
        "server_port": int(port),
        "password": password,
    }
    _apply_hysteria_tls(outbound, params, server)
    _apply_hysteria_obfs(outbound, params)
    name = _clean_name(parsed.fragment, f"hysteria2-{server}:{port}")
    return Node(name=name, scheme="hysteria2", server=server, port=int(port), link=link, outbound=_native_singbox_outbound(outbound))


def _parse_tuic(link: str) -> Node:
    # Разбор TUIC-ссылки: tuic://uuid:password@host:port?congestion_control=&udp_relay_mode=&sni=&alpn=#name
    parsed = urlsplit(link)
    query = parse_qs(parsed.query, keep_blank_values=True)
    params = {k: _first(query, k) for k in query}
    server = parsed.hostname or _get_param(params, "server", "address", "host")
    port = parsed.port or int(_get_param(params, "port", default="0") or 0) or 443
    uuid = unquote(parsed.username or "") or _get_param(params, "uuid", "id")
    password = unquote(parsed.password or "") or _get_param(params, "password", "passwd", "pass")
    if not server or not uuid:
        raise LinkParseError("tuic link must contain server and uuid")
    outbound: dict[str, Any] = {
        "type": "tuic",
        "tag": "proxy",
        "server": server,
        "server_port": int(port),
        "uuid": uuid,
    }
    if password:
        outbound["password"] = password
    congestion = _get_param(params, "congestion_control", "congestion", "cc")  # cubic/bbr/new_reno
    if congestion:
        outbound["congestion_control"] = congestion
    udp_relay = _get_param(params, "udp_relay_mode", "udp_relay", "udpRelayMode")  # native/quic
    if udp_relay:
        outbound["udp_relay_mode"] = udp_relay
    if _to_bool(_get_param(params, "zero_rtt_handshake", "zero_rtt", "reduce_rtt", default="")):
        outbound["zero_rtt_handshake"] = True
    _apply_hysteria_tls(outbound, params, server)  # переиспользуем TLS-хелпер (sni/alpn/insecure)
    name = _clean_name(parsed.fragment, f"tuic-{server}:{port}")
    return Node(name=name, scheme="tuic", server=server, port=int(port), link=link, outbound=_native_singbox_outbound(outbound))


def _apply_hysteria_tls(outbound: dict[str, Any], params: dict[str, str], server: str) -> None:
    tls: dict[str, Any] = {"enabled": True}
    sni = _get_param(params, "sni", "peer", "server_name", "serverName")
    if sni:
        tls["server_name"] = sni
    elif server:
        tls["server_name"] = server
    insecure = _get_param(params, "insecure", "allowInsecure")
    if insecure:
        tls["insecure"] = _to_bool(insecure)
    alpn = _get_param(params, "alpn")
    if alpn:
        tls["alpn"] = [item.strip() for item in alpn.split(",") if item.strip()]
    outbound["tls"] = tls


def _apply_hysteria_obfs(outbound: dict[str, Any], params: dict[str, str]) -> None:
    obfs_type = _get_param(params, "obfs", "obfs_type", "obfsType")
    obfs_password = _get_param(params, "obfs-password", "obfs_password", "obfsPassword", "obfs-pass", "obfsPass")
    if not obfs_type and not obfs_password:
        return
    if obfs_type == "none":
        return
    if str(outbound.get("type") or "") == "hysteria":
        outbound["obfs"] = obfs_password or obfs_type
        return
    outbound["obfs"] = {
        "type": obfs_type or "salamander",
        **({"password": obfs_password} if obfs_password else {}),  # salamander: пароль опционален
    }


def _parse_wireguard_like_link(link: str, scheme: str) -> Node:
    parsed = urlsplit(link)
    query = parse_qs(parsed.query, keep_blank_values=True)
    params = {k: _first(query, k) for k in query}

    if scheme == "warp":
        endpoint = _build_warp_endpoint(params)
        name = _clean_name(parsed.fragment, "AWG/WARP" if endpoint.get("amnezia") else "WARP")
        return Node(name=name, scheme="warp", server="engage.cloudflareclient.com", port=2408, link=link, outbound=_native_singbox_outbound(endpoint), tags=["WARP"])

    server = parsed.hostname or _get_param(params, "server", "endpoint", "address")
    port = parsed.port or int(_get_param(params, "port", default="0") or 0) or 51820
    private_key = _clean_b64_key(unquote(parsed.username or "") or _get_param(params, "private_key", "privateKey", "key"))
    public_key = _clean_b64_key(_get_param(params, "public_key", "publicKey", "peer_public_key", "peerPublicKey", "pbk"))
    address = _split_csv(_get_param(params, "address", "local_address", "localAddress", default="10.0.0.2/32"))
    allowed_ips = _split_csv(_get_param(params, "allowed_ips", "allowedIPs", "allowed", default="0.0.0.0/0,::/0"))
    pre_shared_key = _get_param(params, "pre_shared_key", "preshared_key", "psk", "reserved")
    mtu = int(_get_param(params, "mtu", default="1408") or 1408)

    if not private_key or not public_key or not server:
        raise LinkParseError("wireguard/awg link must contain server, private_key and public_key")

    endpoint = _build_wireguard_endpoint(
        server=server,
        port=port,
        private_key=_clean_b64_key(private_key),
        public_key=_clean_b64_key(public_key),
        address=address,
        allowed_ips=allowed_ips,
        pre_shared_key=pre_shared_key,
        mtu=mtu,
        amnezia=_amnezia_from_params(params) if scheme == "awg" else {},
    )
    name = _clean_name(
        parsed.fragment,
        _wireguard_display_name(server, port, bool(endpoint.get("amnezia")), scheme),
    )
    tags = ["WARP"] if _is_warp_endpoint(server) else []
    return Node(name=name, scheme=scheme, server=server, port=port, link=link, outbound=_native_singbox_outbound(endpoint), tags=tags)


def _parse_wireguard_config(text: str) -> Node:
    sections: dict[str, dict[str, str]] = {}
    current = ""
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or line.startswith(";"):
            continue
        if line.startswith("[") and line.endswith("]"):
            current = line.strip("[]").strip().lower()
            sections.setdefault(current, {})
            continue
        if "=" not in line or not current:
            continue
        key, value = line.split("=", 1)
        sections.setdefault(current, {})[key.strip().lower()] = value.strip()

    interface = sections.get("interface") or {}
    peer = sections.get("peer") or {}
    endpoint_value = peer.get("endpoint", "")
    server, port = _split_endpoint(endpoint_value)
    private_key = interface.get("privatekey", "")
    public_key = peer.get("publickey", "")
    if not server or not private_key or not public_key:
        raise LinkParseError("wireguard config must contain [Interface] PrivateKey and [Peer] Endpoint/PublicKey")

    amnezia = _amnezia_from_params({key: value for key, value in interface.items()})
    endpoint = _build_wireguard_endpoint(
        server=server,
        port=port or 51820,
        private_key=private_key,
        public_key=public_key,
        address=_split_csv(interface.get("address", "10.0.0.2/32")),
        allowed_ips=_split_csv(peer.get("allowedips", "0.0.0.0/0,::/0")),
        pre_shared_key=_clean_b64_key(peer.get("presharedkey", "")),
        mtu=int(interface.get("mtu", "1408") or 1408),
        amnezia=amnezia,
    )
    scheme = "awg" if amnezia else "wireguard"
    return Node(
        name=_wireguard_display_name(server, port or 51820, bool(amnezia), scheme),
        scheme=scheme,
        server=server,
        port=port or 51820,
        link=text,
        outbound=_native_singbox_outbound(endpoint),
        tags=["WARP"] if _is_warp_endpoint(server) else [],
    )


def _build_warp_endpoint(params: dict[str, str]) -> dict[str, Any]:
    endpoint: dict[str, Any] = {
        "type": "warp",
        "tag": "proxy",
        "listen_port": int(_get_param(params, "listen_port", "listenPort", default="10000") or 10000),
        "udp_timeout": _get_param(params, "udp_timeout", "udpTimeout", default="5m0s") or "5m0s",
    }
    amnezia = _amnezia_from_params(params)
    if amnezia:
        endpoint["amnezia"] = amnezia
    profile: dict[str, Any] = {"detour": "direct"}
    for source, target in (("id", "id"), ("private_key", "private_key"), ("privateKey", "private_key"), ("auth_token", "auth_token"), ("authToken", "auth_token")):
        value = _get_param(params, source)
        if value:
            profile[target] = value
    endpoint["profile"] = profile
    return endpoint


def _wireguard_display_name(server: str, port: int, amnezia: bool, scheme: str = "wireguard") -> str:
    return f"{server}:{int(port or 51820)}"


def _is_warp_endpoint(server: str) -> bool:
    host = str(server or "").strip().lower().strip(".")
    if host == "engage.cloudflareclient.com" or host.endswith(".cloudflareclient.com"):
        return True
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        return False
    return any(
        address in network
        for network in (
            ipaddress.ip_network("162.159.192.0/24"),
            ipaddress.ip_network("162.159.193.0/24"),
            ipaddress.ip_network("188.114.96.0/20"),
            ipaddress.ip_network("2606:4700:d0::/48"),
            ipaddress.ip_network("2606:4700:d1::/48"),
        )
    )


def _build_wireguard_endpoint(
    *,
    server: str,
    port: int,
    private_key: str,
    public_key: str,
    address: list[str],
    allowed_ips: list[str],
    pre_shared_key: str = "",
    mtu: int = 1408,
    amnezia: dict[str, Any] | None = None,
) -> dict[str, Any]:
    peer: dict[str, Any] = {
        "address": server,
        "port": int(port),
        "public_key": public_key,
        "allowed_ips": allowed_ips or ["0.0.0.0/0", "::/0"],
    }
    if pre_shared_key:
        peer["pre_shared_key"] = _clean_b64_key(pre_shared_key)
    endpoint: dict[str, Any] = {
        "type": "wireguard",
        "tag": "proxy",
        "mtu": int(mtu or 1408),
        "address": address or ["10.0.0.2/32"],
        "private_key": _clean_b64_key(private_key),
        "listen_port": 10000,
        "peers": [peer],
        "udp_timeout": "5m0s",
    }
    if amnezia:
        endpoint["amnezia"] = amnezia
    return endpoint


def _amnezia_from_params(params: dict[str, str]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key in ("jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4", "itime"):
        value = _get_param(params, key, key.upper())
        if value:
            try:
                result[key] = int(value)
            except ValueError:
                result[key] = value
    for key in ("i1", "i2", "i3", "i4", "i5", "j1", "j2", "j3"):
        value = _get_param(params, key, key.upper())
        if value:
            result[key] = value
    return result


def _split_csv(value: str) -> list[str]:
    return [item.strip() for item in str(value or "").split(",") if item.strip()]


def _split_endpoint(value: str) -> tuple[str, int]:
    text = str(value or "").strip()
    if not text:
        return "", 0
    if text.startswith("[") and "]:" in text:
        host, _, port_text = text[1:].partition("]:")
        return host, int(port_text or 0)
    if ":" in text:
        host, _, port_text = text.rpartition(":")
        return host.strip(), int(port_text or 0)
    return text, 0


def _clean_b64_key(value: str) -> str:
    return str(value or "").strip().replace(" ", "+")
