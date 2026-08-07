from __future__ import annotations

from pathlib import Path
import re
import shlex
from typing import Any

from .openvpn_normalization import (
    normalize_auth_digest,
    normalize_data_cipher,
    normalize_tls_cipher_suites,
)


_INLINE_BLOCK_RE = re.compile(
    r"(?ims)^\s*<(?P<tag>ca|cert|key|tls-auth|tls-crypt|tls-crypt-v2|auth-user-pass|askpass)>\s*\r?\n"
    r"(?P<body>.*?)^\s*</(?P=tag)>\s*$"
)
_UNSAFE_UNSUPPORTED_DIRECTIVES = {
    "http-proxy-user-pass",
    "pkcs12",
    "secret",
}


def looks_like_openvpn_config(text: str) -> bool:
    lowered = str(text or "").lower()
    return bool(
        re.search(r"(?m)^\s*(?:--)?remote\s+\S+", lowered)
        and (
            re.search(r"(?m)^\s*(?:--)?client(?:\s|$)", lowered)
            or re.search(r"(?m)^\s*(?:--)?tls-client(?:\s|$)", lowered)
            or "<ca>" in lowered
        )
    )


def parse_openvpn_config(
    text: str,
    *,
    source_path: Path | None = None,
) -> tuple[dict[str, Any], list[str], str]:
    """Translate a client .ovpn profile to sing-box-extended OpenVPN options.

    Only fields implemented by the extended core are emitted. Referenced files
    are restricted to the profile directory and embedded into the returned
    configuration so imported profiles remain portable.
    """
    if not looks_like_openvpn_config(text):
        raise ValueError("not an OpenVPN client profile")

    text = str(text).replace("\r\n", "\n").replace("\r", "\n")
    inline: dict[str, str] = {}

    def collect_block(match: re.Match[str]) -> str:
        tag = match.group("tag").lower()
        body = match.group("body").strip("\r\n")
        inline[tag] = body + ("\n" if body else "")
        return ""

    directives_text = _INLINE_BLOCK_RE.sub(collect_block, text)
    directives: list[tuple[str, list[str]]] = []
    for line_number, raw_line in enumerate(directives_text.splitlines(), start=1):
        stripped = raw_line.strip()
        if not stripped or stripped.startswith(("#", ";")):
            continue
        try:
            tokens = shlex.split(stripped, comments=False, posix=True)
        except ValueError as exc:
            raise ValueError(f"OpenVPN line {line_number}: {exc}") from exc
        if not tokens:
            continue
        key = tokens[0].lstrip("-").strip().lower()
        directives.append((key, tokens[1:]))

    by_key: dict[str, list[list[str]]] = {}
    for key, values in directives:
        by_key.setdefault(key, []).append(values)

    for key in _UNSAFE_UNSUPPORTED_DIRECTIVES:
        if key in by_key:
            raise ValueError(f"OpenVPN directive `{key}` is not supported by sing-box extended")
    dev = _last_arg(by_key, "dev").lower()
    if dev.startswith("tap"):
        raise ValueError("OpenVPN TAP profiles are not supported; a TUN profile is required")
    for key in ("compress", "comp-lzo"):
        value = _last_arg(by_key, key).lower()
        if value and value not in {"no", "disable", "stub", "stub-v2"}:
            raise ValueError(f"OpenVPN compression `{value}` is not supported")

    global_proto = _normalize_proto(_last_arg(by_key, "proto") or "udp")
    remotes: list[tuple[str, dict[str, Any]]] = []
    for values in by_key.get("remote", []):
        if not values:
            continue
        server = values[0].strip()
        if not server:
            continue
        port = _positive_port(values[1] if len(values) > 1 else "1194")
        remote_proto = _normalize_proto(values[2]) if len(values) > 2 else global_proto
        remotes.append((remote_proto, {"server": server, "server_port": port}))
    if not remotes:
        raise ValueError("OpenVPN profile does not contain a usable `remote` server")
    tcp_remotes = [remote for remote in remotes if remote[0] == "tcp"]
    selected = tcp_remotes or remotes
    proto = selected[0][0]
    if proto != "tcp":
        raise ValueError(
            "OpenVPN over UDP is not supported by this core; import the TCP variant of this profile"
        )
    servers = [remote[1] for remote in selected]

    native: dict[str, Any] = {
        "type": "openvpn",
        "tag": "proxy",
        "system": False,
        "name": "openvpn0",
        "servers": servers,
        "proto": proto,
    }
    if "auth-user-pass" in by_key or "auth-user-pass" in inline:
        native["lumen_requires_user_auth"] = True

    proxy = _parse_openvpn_proxy(text, by_key, source_path)
    if proxy:
        native["lumen_proxy"] = proxy

    cipher = _select_cipher(by_key)
    if cipher:
        native["cipher"] = cipher
    auth = _last_arg(by_key, "auth")
    if auth and auth.lower() != "none":
        normalized_auth = normalize_auth_digest(auth)
        if normalized_auth:
            native["auth"] = normalized_auth

    credentials = inline.get("auth-user-pass", "")
    auth_user_pass = _last_values(by_key, "auth-user-pass")
    if not credentials and auth_user_pass:
        credentials = _read_profile_resource(auth_user_pass[0], source_path)
    if credentials:
        lines = [line.strip() for line in credentials.splitlines() if line.strip()]
        if lines:
            native["username"] = lines[0]
        if len(lines) > 1:
            native["password"] = lines[1]

    key_password = _openvpn_key_password(text, inline)
    askpass_values = _last_values(by_key, "askpass")
    if not key_password and askpass_values:
        key_password = _read_profile_resource(askpass_values[0], source_path).strip()
    if key_password:
        native["key_password"] = key_password.splitlines()[0].strip()

    for directive, native_key in (
        ("tls-auth", "tls_auth"),
        ("tls-crypt", "tls_crypt"),
        ("tls-crypt-v2", "tls_crypt"),
    ):
        content = inline.get(directive, "")
        values = _last_values(by_key, directive)
        if not content and values:
            content = _read_profile_resource(values[0], source_path)
        if content:
            native[native_key] = content
        if directive == "tls-auth" and len(values) > 1:
            native["key_direction"] = _key_direction(values[1])
        if directive == "tls-auth" and content and len(values) <= 1:
            native.setdefault("key_direction", -1)
        if directive == "tls-crypt-v2" and (content or values):
            native["tls_crypt_v2"] = True

    direction = _last_arg(by_key, "key-direction")
    if direction:
        native["key_direction"] = _key_direction(direction)

    for directive, native_key in (
        ("connect-retry", "reconnect_delay"),
        ("ping", "ping_interval"),
        ("ping-restart", "ping_restart"),
    ):
        value = _last_arg(by_key, directive)
        if value:
            native[native_key] = _duration_seconds(value, directive)

    keepalive = _last_values(by_key, "keepalive")
    if keepalive:
        native.setdefault("ping_interval", _duration_seconds(keepalive[0], "keepalive"))
    if len(keepalive) > 1:
        native.setdefault("ping_restart", _duration_seconds(keepalive[1], "keepalive"))

    tls: dict[str, Any] = {}
    for directive, native_key in (("cert", "certificate"), ("key", "key"), ("ca", "ca")):
        content = inline.get(directive, "")
        values = _last_values(by_key, directive)
        if not content and values:
            content = _read_profile_resource(values[0], source_path)
        if content:
            tls[native_key] = content
    # The bundled core maps this field to Go TLS <=1.2 cipher suites;
    # tls-ciphersuites is a TLS 1.3 OpenVPN option and must not be copied here.
    tls_ciphers = normalize_tls_cipher_suites(_last_arg(by_key, "tls-cipher"))
    if tls_ciphers:
        tls["cipher_suites"] = tls_ciphers
    verify_values = _last_values(by_key, "verify-x509-name")
    if verify_values:
        tls["verify_x509_name"] = verify_values[0]
        if len(verify_values) > 1:
            tls["verify_x509_name_mode"] = _verify_name_mode(verify_values[1])
    if not tls.get("ca"):
        raise ValueError("OpenVPN profile does not contain a CA certificate supported by this core")
    native["tls"] = tls

    dns_servers: list[str] = []
    for values in by_key.get("dhcp-option", []):
        if len(values) >= 2 and values[0].upper() in {"DNS", "DNS6"}:
            address = values[1].strip()
            if address and address not in dns_servers:
                dns_servers.append(address)

    profile_name = source_path.stem if source_path is not None else f"OpenVPN {servers[0]['server']}"
    return native, dns_servers, profile_name


def _last_values(by_key: dict[str, list[list[str]]], key: str) -> list[str]:
    values = by_key.get(key)
    return values[-1] if values else []


def _last_arg(by_key: dict[str, list[list[str]]], key: str) -> str:
    values = _last_values(by_key, key)
    return str(values[0]).strip() if values else ""


def _normalize_proto(value: str) -> str:
    proto = str(value or "udp").strip().lower()
    if proto.startswith("udp"):
        return "udp"
    if proto in {"tcp", "tcp-client", "tcp4", "tcp4-client", "tcp6", "tcp6-client"}:
        return "tcp"
    raise ValueError(f"unsupported OpenVPN transport `{value}`")


def _positive_port(value: str) -> int:
    try:
        port = int(str(value).strip())
    except (TypeError, ValueError) as exc:
        raise ValueError(f"invalid OpenVPN remote port `{value}`") from exc
    if port <= 0 or port > 65535:
        raise ValueError(f"invalid OpenVPN remote port `{value}`")
    return port


def _select_cipher(by_key: dict[str, list[list[str]]]) -> str:
    candidates: list[str] = []
    for key in ("data-ciphers", "ncp-ciphers", "cipher", "data-ciphers-fallback"):
        value = _last_arg(by_key, key)
        if value:
            candidates.extend(value.split(":"))
    for value in candidates:
        normalized = normalize_data_cipher(value)
        if normalized:
            return normalized
    return ""


def _read_profile_resource(value: str, source_path: Path | None) -> str:
    if source_path is None:
        raise ValueError(f"OpenVPN resource `{value}` can only be resolved when importing an .ovpn file")
    base = source_path.resolve().parent
    candidate = Path(str(value).strip().strip('"\''))
    if not candidate.is_absolute():
        candidate = base / candidate
    candidate = candidate.resolve()
    try:
        candidate.relative_to(base)
    except ValueError as exc:
        raise ValueError(f"OpenVPN resource must be inside the profile directory: {value}") from exc
    if not candidate.is_file():
        raise ValueError(f"OpenVPN resource was not found: {candidate.name}")
    if candidate.stat().st_size > 1024 * 1024:
        raise ValueError(f"OpenVPN resource is too large: {candidate.name}")
    return candidate.read_text(encoding="utf-8", errors="replace")


def _key_direction(value: str) -> int:
    normalized = str(value or "").strip().lower()
    if normalized in {"0", "1"}:
        return int(normalized)
    if normalized in {"bidirectional", "bi", "-1"}:
        return -1
    raise ValueError(f"invalid OpenVPN key direction `{value}`")


def _duration_seconds(value: str, directive: str) -> str:
    try:
        seconds = int(float(str(value).strip()))
    except (TypeError, ValueError) as exc:
        raise ValueError(f"invalid OpenVPN `{directive}` value: {value}") from exc
    if seconds < 0:
        raise ValueError(f"invalid OpenVPN `{directive}` value: {value}")
    return f"{seconds}s"


def _verify_name_mode(value: str) -> str:
    normalized = str(value or "name").strip().lower()
    if normalized in {"name", "name-prefix", "name-suffix"}:
        return normalized
    if normalized == "subject":
        raise ValueError(
            "OpenVPN verify-x509-name mode `subject` is not supported by the bundled core"
        )
    raise ValueError(f"invalid OpenVPN verify-x509-name mode `{value}`")


def openvpn_requires_user_auth(text: str) -> bool:
    inline_auth = False

    def strip_inline(match: re.Match[str]) -> str:
        nonlocal inline_auth
        inline_auth = inline_auth or match.group("tag").lower() == "auth-user-pass"
        return ""

    directives_text = _INLINE_BLOCK_RE.sub(strip_inline, str(text or ""))
    if inline_auth:
        return True
    for raw_line in directives_text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith(("#", ";")):
            continue
        try:
            tokens = shlex.split(line, comments=False, posix=True)
        except ValueError:
            continue
        if tokens and tokens[0].lstrip("-").strip().lower() == "auth-user-pass":
            return True
    return False


def _openvpn_comment_tokens(text: str, prefix: str) -> list[str]:
    marker = f"# {prefix} "
    value = ""
    for line in str(text or "").splitlines():
        stripped = line.strip()
        if stripped.startswith(marker):
            value = stripped[len(marker):].strip()
    if not value:
        return []
    try:
        return shlex.split(value, comments=False, posix=True)
    except ValueError as exc:
        raise ValueError(f"invalid OpenVPN `{prefix}` comment: {exc}") from exc


def _openvpn_key_password(text: str, inline: dict[str, str]) -> str:
    inline_password = next(
        (line.strip() for line in inline.get("askpass", "").splitlines() if line.strip()),
        "",
    )
    if inline_password:
        return inline_password
    values = _openvpn_comment_tokens(text, "lumen-key-password")
    return values[0] if values else ""


def _parse_openvpn_proxy(
    text: str,
    by_key: dict[str, list[list[str]]],
    source_path: Path | None,
) -> dict[str, Any] | None:
    obfs = _openvpn_comment_tokens(text, "lumen-proxy")
    auth = _openvpn_comment_tokens(text, "lumen-proxy-auth")
    http = _last_values(by_key, "http-proxy")
    socks = _last_values(by_key, "socks-proxy")
    if obfs and obfs[0].lower() in {"obfs2", "obfs2-legacy", "obfs3"}:
        raise ValueError(
            f"OpenVPN proxy `{obfs[0]}` requires the Android obfs relay and is not supported on Windows"
        )
    if http:
        proxy_type, endpoint = "http", http
    elif socks:
        proxy_type, endpoint = "socks", socks
    else:
        return None
    if not endpoint[0].strip():
        raise ValueError(f"OpenVPN proxy `{proxy_type}` is missing a server address")
    port = _positive_port(endpoint[1] if len(endpoint) > 1 else "")
    if len(endpoint) > 2:
        credentials = _read_profile_resource(endpoint[2], source_path)
        lines = [line.strip() for line in credentials.splitlines() if line.strip()]
        if lines:
            auth = lines[:2]
    result: dict[str, Any] = {
        "type": proxy_type,
        "server": endpoint[0].strip(),
        "server_port": port,
    }
    if auth:
        result["username"] = auth[0]
    if len(auth) > 1:
        result["password"] = auth[1]
    return result
