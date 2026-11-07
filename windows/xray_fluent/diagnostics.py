"""Allowlisted, scrubbed diagnostics with no implicit machine fingerprinting."""
from __future__ import annotations
from collections import deque
from collections.abc import Callable, Iterable
from datetime import datetime, timezone
import io
import json
import os
import platform
from pathlib import Path
import re
import socket
import sys
import tempfile
import zipfile
from .constants import APP_VERSION, LOG_DIR
from .data_paths import get_install_id
from .models import AppState
from .secret_scrubber import is_sensitive_key, safe_text, scrub_text, scrub_value

_DOMAIN_LOG_FILES = {"errors": "errors.log", "core": "core.log", "app": "app.log", "traffic": "traffic.log"}
_LOG_ROTATIONS = 3
_DEFAULT_INCLUDE = {"errors": True, "core": True, "app": True, "traffic": True,
                    "state": True, "recent": True, "network": False, "crash": False}
_MAX_LOG_BYTES = 2 * 1024 * 1024
_MAX_ENTRY_BYTES = 3 * 1024 * 1024
_MAX_BUNDLE_BYTES = 48 * 1024 * 1024
_MAX_MEMBERS = 32
_VERSION = re.compile(r"\d+\.\d+(?:\.\d+)?(?:[-+._](?:alpha|beta|rc|dev)\d*)?\Z")
_ENGINE_VERSION = re.compile(r"\d{1,4}(?:\.\d{1,5}){1,3}(?:[-+][0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?")
_SETTING_ENUMS = {
    "language": {"ru", "en", "de", "fr", "es", "zh", "pt", "ja", "ko", "tr", "uk"},
    "theme": {"dark", "light", "system"}, "theme_mode": {"dark", "light", "system"},
    "engine": {"xray", "sing-box", "singbox"}, "core_type": {"xray", "sing-box", "singbox"},
    "proxy_mode": {"proxy", "system", "tun", "hybrid"},
}
_SETTING_BOOLS = {"auto_connect", "tun_enabled", "diagnostics_upload_enabled", "minimize_to_tray",
                  "auto_start", "start_minimized", "use_system_proxy"}

class DiagnosticsCancelled(RuntimeError):
    """Generation was cancelled or its consent generation was invalidated."""

def _normalize_include(include: dict | None) -> dict[str, bool]:
    # {} selects NONE. Truthy strings/integers never silently opt in.
    if include is None:
        return dict(_DEFAULT_INCLUDE)
    return {key: include.get(key) is True for key in _DEFAULT_INCLUDE}

def _should_redact(key: object) -> bool:
    return is_sensitive_key(key)

def _redact(value):
    return scrub_value(value)

def _safe_version(value: object) -> str:
    return value if isinstance(value, str) and _VERSION.fullmatch(value) else "unknown"


def _safe_install_id(value: object) -> str | None:
    if isinstance(value, str) and re.fullmatch(
        r"(?:[0-9a-f]{32}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})",
        value, re.IGNORECASE,
    ):
        return value
    return None


def _sanitize_runtime(value: object) -> dict:
    source = value if isinstance(value, dict) else {}
    safe = {}
    architecture = source.get("architecture")
    if architecture in ("AMD64", "ARM64", "x86_64", "aarch64", "x86", "i386", "i686"):
        safe["architecture"] = architecture
    version = source.get("windows_version")
    if isinstance(version, str) and re.fullmatch(r"\d{1,3}\.\d{1,3}\.\d{1,8}", version):
        safe["windows_version"] = version
    engines = source.get("engines")
    if isinstance(engines, dict):
        safe["engines"] = {
            name: version if isinstance(version, str) and len(version) <= 80 and _ENGINE_VERSION.fullmatch(version) else "unknown"
            for name in ("xray", "sing-box") for version in (engines.get(name),)
        }
    return safe


def collect_runtime_context(state: AppState) -> dict:
    """Technical versions only; never include executable paths or host names."""
    from .engines.xray import get_xray_version
    from .engines.singbox import get_singbox_version

    context = {"architecture": platform.machine(), "engines": {}}
    if sys.platform == "win32":
        version = sys.getwindowsversion()
        context["windows_version"] = f"{version.major}.{version.minor}.{version.build}"
    settings = getattr(state, "settings", None)
    for name, probe, key in (("xray", get_xray_version, "xray_path"),
                             ("sing-box", get_singbox_version, "singbox_path")):
        try:
            output = probe(getattr(settings, key, "") or "")
            match = _ENGINE_VERSION.search(output or "")
            context["engines"][name] = match.group(0) if match else "unknown"
        except Exception:
            context["engines"][name] = "unknown"
    return _sanitize_runtime(context)

def _sanitize_state(state_dict: dict) -> dict:
    """Counts and typed settings only: never names, links or unknown fields."""
    if not isinstance(state_dict, dict):
        return {}
    safe = {}
    for key in ("nodes", "subscriptions", "routing_rules", "groups"):
        count_key = key.removesuffix("s") + "_count"
        value, previous = state_dict.get(key), state_dict.get(count_key)
        safe[count_key] = len(value) if isinstance(value, list) else (
            previous if type(previous) is int and 0 <= previous <= 10_000_000 else 0)
    settings, safe_settings = state_dict.get("settings"), {}
    if isinstance(settings, dict):
        for key, choices in _SETTING_ENUMS.items():
            value = settings.get(key)
            if isinstance(value, str) and value in choices:
                safe_settings[key] = value
        for key in _SETTING_BOOLS:
            if type(settings.get(key)) is bool:
                safe_settings[key] = settings[key]
    safe["settings"] = safe_settings
    return safe

def _collect_log_files(log_dir: Path, domains) -> list[Path]:
    found = []
    for domain in domains:
        name = _DOMAIN_LOG_FILES.get(domain)
        if name is None:
            continue
        for suffix in ("", *(f".{index}" for index in range(1, _LOG_ROTATIONS + 1))):
            path = Path(log_dir) / (name + suffix)
            if path.is_file() and not path.is_symlink():
                found.append(path)
    return found

def _probe_tcp(family: int, address: str) -> bool:
    """Reachability of a fixed TCP endpoint, not a guarantee of general Internet access."""
    try:
        with socket.socket(family, socket.SOCK_STREAM) as connection:
            connection.settimeout(1.0)
            connection.connect((address, 443))
        return True
    except OSError:
        return False


def _system_proxy_status() -> tuple[bool | None, bool | None]:
    if sys.platform != "win32":
        return None, None
    try:
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Internet Settings") as key:
            try:
                enabled = bool(winreg.QueryValueEx(key, "ProxyEnable")[0])
            except OSError:
                enabled = False
            try:
                pac_present = bool(winreg.QueryValueEx(key, "AutoConfigURL")[0])
            except OSError:
                pac_present = False
        return enabled, pac_present
    except OSError:
        return None, None


def collect_network_context() -> dict:
    """Explicitly selected probes; retain only booleans, never network addresses."""
    from .windows_network import query_windows_route_context
    route = query_windows_route_context() if sys.platform == "win32" else None
    proxy_enabled, pac_present = _system_proxy_status()
    return {
        "probe_performed": True,
        "probe_target": "cloudflare_tcp_443",
        "ipv4_internet": _probe_tcp(socket.AF_INET, "1.1.1.1"),
        "ipv6_internet": _probe_tcp(socket.AF_INET6, "2606:4700:4700::1111") if socket.has_ipv6 else False,
        "ipv6_supported": bool(socket.has_ipv6), "system_dns": [],
        "tun_active": route.tun_active if route is not None else None,
        "system_proxy_enabled": proxy_enabled, "system_pac_present": pac_present,
        "proxy_info": {
            "env_proxies": {scheme: any(name in os.environ for name in (scheme + "_proxy", scheme.upper() + "_PROXY"))
                            for scheme in ("http", "https", "all")},
            "windows_proxy_enable": None, "windows_proxy_server": None, "windows_auto_config_url": None,
        },
        "connected_adapters": [],
    }

def _sanitize_network(value: object) -> dict:
    source = value if isinstance(value, dict) else {}
    safe = {key: source.get(key) if type(source.get(key)) is bool else None
            for key in ("probe_performed", "ipv4_internet", "ipv6_internet", "ipv6_supported",
                        "tun_active", "system_proxy_enabled", "system_pac_present")}
    if source.get("probe_target") == "cloudflare_tcp_443":
        safe["probe_target"] = "cloudflare_tcp_443"
    proxy_info = source.get("proxy_info")
    proxies = proxy_info.get("env_proxies", {}) if isinstance(proxy_info, dict) else source.get("proxy_env_present", {})
    safe["proxy_env_present"] = {key: type(proxies.get(key)) is bool and proxies[key]
                                 for key in ("http", "https", "all")} if isinstance(proxies, dict) else {}
    return safe

def _sanitize_meta(value: object, *, log_files: list[str]) -> dict:
    source = value if isinstance(value, dict) else {}
    sections = source.get("sections", [])
    sections = [key for key in _DEFAULT_INCLUDE if isinstance(sections, list) and key in sections]
    timestamp = source.get("timestamp")
    try:
        parsed = datetime.fromisoformat(timestamp) if isinstance(timestamp, str) else None
        timestamp = parsed.isoformat() if parsed is not None else ""
    except (ValueError, OverflowError):
        timestamp = ""
    platform_name = source.get("platform")
    safe = {"app_version": _safe_version(source.get("app_version")), "timestamp": timestamp,
            "platform": platform_name if platform_name in ("win32", "linux", "darwin", "Windows", "Linux", "Darwin") else "unknown",
            "python": _safe_version(source.get("python")), "sections": sections, "log_files": log_files}
    install_id = _safe_install_id(source.get("install_id"))
    if install_id is not None:
        safe["install_id"] = install_id
    if "runtime" in source:
        safe["runtime"] = _sanitize_runtime(source["runtime"])
    if "network" in sections:
        safe["network"] = _sanitize_network(source.get("network"))
    return safe

def _allowed_members(sections: Iterable[str]) -> set[str]:
    selected = set(sections)
    allowed = {"meta.json"}
    for domain, name in _DOMAIN_LOG_FILES.items():
        if domain in selected:
            allowed.update(f"logs/{name}{suffix}" for suffix in ("", ".1", ".2", ".3"))
    if "state" in selected:
        allowed.add("state_redacted.json")
    if "recent" in selected:
        allowed.add("recent_logs.txt")
    if "crash" in selected:
        allowed.add("logs/faulthandler.log")
    return allowed

def sanitize_diagnostic_zip(data: bytes, *, cancelled: Callable[[], bool] | None = None) -> bytes:
    """Rebuild old ZIPs from allowlisted selected entries; never extract files.

    Reject oversized/duplicate/encrypted archives. Drop comments/ZIP extras,
    unknown paths, raw state, identifying metadata and unselected sections.
    """
    def check() -> None:
        if cancelled is not None and cancelled():
            raise DiagnosticsCancelled("Diagnostic generation cancelled")
    check()
    if len(data) > _MAX_BUNDLE_BYTES:
        raise ValueError("Diagnostic archive exceeds size limit")
    output = io.BytesIO()
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        members = archive.infolist()
        if len(members) > _MAX_MEMBERS or len({info.filename for info in members}) != len(members):
            raise ValueError("Invalid diagnostic archive members")
        if any(info.flag_bits & 1 or info.file_size > _MAX_ENTRY_BYTES for info in members):
            raise ValueError("Unsupported diagnostic archive member")
        if sum(info.file_size for info in members) > _MAX_BUNDLE_BYTES:
            raise ValueError("Diagnostic contents exceed size limit")
        metadata = json.loads(archive.read("meta.json").decode("utf-8"))
        safe_meta = _sanitize_meta(metadata, log_files=[])
        allowed = _allowed_members(safe_meta["sections"])
        files = []
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as cleaned:
            for info in members:
                check()
                if info.filename not in allowed or info.filename == "meta.json" or info.is_dir():
                    continue
                raw = archive.read(info).decode("utf-8", errors="replace")
                text = json.dumps(_sanitize_state(json.loads(raw)), ensure_ascii=True, indent=2) if info.filename == "state_redacted.json" else scrub_text(raw)
                cleaned.writestr(info.filename, text)
                if info.filename.startswith("logs/"):
                    files.append(info.filename.removeprefix("logs/"))
            safe_meta["log_files"] = files
            cleaned.writestr("meta.json", json.dumps(safe_meta, ensure_ascii=True, indent=2))
    check()
    return output.getvalue()

def _read_log(path: Path) -> str:
    with path.open("rb") as stream:
        data = stream.read(_MAX_LOG_BYTES + 1)
    # Start at BEGIN rather than potentially tailing into a private-key body.
    text = scrub_text(data[:_MAX_LOG_BYTES].decode("utf-8", errors="replace"))
    return text + ("\n[TRUNCATED diagnostic log]" if len(data) > _MAX_LOG_BYTES else "")

def export_diagnostics(zip_path: Path, state: AppState, logs: Iterable[str],
                       include: dict | None = None, *, cancelled: Callable[[], bool] | None = None) -> Path:
    """Atomic selected export. {} is metadata-only; None retains six core sections.

    Network capabilities and crash logs require explicit True flags. Consent
    generation changes cancel work; a callback can also cancel UI teardown.
    """
    from .diagnostics_uploader import get_upload_epoch, mark_bundle_generation
    epoch = get_upload_epoch()
    zip_path = Path(zip_path)
    mark_bundle_generation(zip_path, epoch)
    def check() -> None:
        if get_upload_epoch() != epoch or (cancelled is not None and cancelled()):
            raise DiagnosticsCancelled("Diagnostic generation cancelled")
    check()
    flags = _normalize_include(include)
    files = _collect_log_files(LOG_DIR, [domain for domain in _DOMAIN_LOG_FILES if flags[domain]])
    crash = Path(LOG_DIR) / "faulthandler.log"
    if flags["crash"] and crash.is_file() and not crash.is_symlink():
        files.append(crash)
    metadata = {"app_version": APP_VERSION, "install_id": get_install_id(), "timestamp": datetime.now(timezone.utc).isoformat(),
                "platform": sys.platform, "python": ".".join(str(part) for part in sys.version_info[:3]),
                "sections": [key for key, selected in flags.items() if selected]}
    metadata["runtime"] = collect_runtime_context(state)
    if flags["network"]:
        metadata["network"] = collect_network_context()
    zip_path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix=".lumen-diagnostic-", suffix=".tmp", dir=zip_path.parent, delete=False) as stream:
            temporary = Path(stream.name)
        written_files = []
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            check()
            if flags["state"]:
                archive.writestr("state_redacted.json", json.dumps(_sanitize_state(state.to_dict()), ensure_ascii=True, indent=2))
            if flags["recent"]:
                recent = deque(logs, maxlen=2000)
                archive.writestr("recent_logs.txt", scrub_text("\n".join(safe_text(line) for line in recent)))
            for path in files:
                check()
                try:
                    text = _read_log(path)
                except OSError:
                    continue
                archive.writestr(f"logs/{path.name}", text)
                written_files.append(path.name)
            archive.writestr("meta.json", json.dumps(_sanitize_meta(metadata, log_files=written_files), ensure_ascii=True, indent=2))
        check()
        temporary.replace(zip_path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    return zip_path
