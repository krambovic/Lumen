from __future__ import annotations

from pathlib import Path
import re
import shlex
from typing import Any


_INLINE_BLOCK_RE = re.compile(
    r"(?ims)^\s*<(?P<tag>ca|cert|key|tls-auth|tls-crypt|tls-crypt-v2|auth-user-pass)>\s*\r?\n"
    r"(?P<body>.*?)^\s*</(?P=tag)>\s*$"
)
_SUPPORTED_CIPHERS = {
    "AES-128-GCM",
    "AES-192-GCM",
    "AES-256-GCM",
    "AES-128-CBC",
    "AES-192-CBC",
    "AES-256-CBC",
    "CHACHA20-POLY1305",
}
_UNSAFE_UNSUPPORTED_DIRECTIVES = {
    "askpass",
    "http-proxy",
    "http-proxy-user-pass",
    "pkcs12",
    "secret",
    "socks-proxy",
    "socks-proxy-retry",
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
    servers: list[dict[str, Any]] = []
    remote_protos: set[str] = set()
    for values in by_key.get("remote", []):
        if not values:
            continue
        server = values[0].strip()
        if not server:
            continue
        port = _positive_port(values[1] if len(values) > 1 else "1194")
        remote_proto = _normalize_proto(values[2]) if len(values) > 2 else global_proto
        remote_protos.add(remote_proto)
        servers.append({"server": server, "server_port": port})
    if not servers:
        raise ValueError("OpenVPN profile does not contain a usable `remote` server")
    if len(remote_protos) > 1:
        raise ValueError("OpenVPN profile mixes TCP and UDP remotes, which this core cannot represent")
    proto = next(iter(remote_protos), global_proto)

    native: dict[str, Any] = {
        "type": "openvpn",
        "tag": "proxy",
        "system": False,
        "name": "openvpn0",
        "servers": servers,
        "proto": proto,
    }

    cipher = _select_cipher(by_key)
    if cipher:
        native["cipher"] = cipher
    auth = _last_arg(by_key, "auth")
    if auth and auth.lower() != "none":
        native["auth"] = auth.upper()

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

    tls: dict[str, Any] = {}
    for directive, native_key in (("cert", "certificate"), ("key", "key"), ("ca", "ca")):
        content = inline.get(directive, "")
        values = _last_values(by_key, directive)
        if not content and values:
            content = _read_profile_resource(values[0], source_path)
        if content:
            tls[native_key] = content
    tls_ciphers: list[str] = []
    for directive in ("tls-cipher", "tls-ciphersuites"):
        value = _last_arg(by_key, directive)
        if value:
            tls_ciphers.extend(item for item in value.split(":") if item)
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
    explicit = _last_arg(by_key, "cipher")
    if explicit:
        candidates.append(explicit)
    for key in ("data-ciphers", "ncp-ciphers", "data-ciphers-fallback"):
        value = _last_arg(by_key, key)
        if value:
            candidates.extend(value.split(":"))
    for value in candidates:
        normalized = value.strip().upper()
        if normalized in _SUPPORTED_CIPHERS:
            return normalized
    if candidates:
        raise ValueError(f"OpenVPN cipher is not supported by sing-box extended: {candidates[0]}")
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
    if normalized in {"bidirectional", "-1"}:
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
    normalized = str(value or "exact").strip().lower()
    return {
        "name": "exact",
        "subject": "exact",
        "name-prefix": "name-prefix",
        "name-suffix": "name-suffix",
        "exact": "exact",
    }.get(normalized, "exact")
