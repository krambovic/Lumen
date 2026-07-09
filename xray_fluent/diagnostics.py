from __future__ import annotations

from datetime import datetime, timezone
import json
from pathlib import Path
import platform
import zipfile

from .constants import APP_VERSION, LOG_DIR
from .data_paths import get_install_id
from .models import AppState
from .subprocess_utils import decode_output


REDACT_KEYS = {
    "id",
    "password",
    "pass",
    "token",
    "publicKey",
    "privateKey",
    "shortId",
    "sid",
    "uuid",
}

_DOMAIN_LOG_FILES = {
    "errors": "errors.log",
    "core": "core.log",
    "app": "app.log",
    "traffic": "traffic.log",
}
_LOG_ROTATIONS = 3 

_DEFAULT_INCLUDE = {
    "errors": True,
    "core": True,
    "app": True,
    "traffic": True,
    "state": True,
    "recent": True,
}


def _normalize_include(include: dict | None) -> dict:
    if not include:
        return dict(_DEFAULT_INCLUDE)
    return {key: bool(include.get(key, False)) for key in _DEFAULT_INCLUDE}


def _redact(value):
    if isinstance(value, dict):
        redacted = {}
        for key, item in value.items():
            if key in REDACT_KEYS:
                redacted[key] = "***"
            else:
                redacted[key] = _redact(item)
        return redacted
    if isinstance(value, list):
        return [_redact(item) for item in value]
    return value


def _sanitize_state(state_dict: dict) -> dict:
    """Redact secrets and reduce nodes/subscriptions to names only"""
    safe = _redact(state_dict)
    nodes = safe.get("nodes")
    if isinstance(nodes, list):
        safe["nodes"] = [
            {"name": item.get("name", "")}
            for item in nodes
            if isinstance(item, dict)
        ]
    subs = safe.get("subscriptions")
    if isinstance(subs, list):
        safe["subscriptions"] = [
            {"name": item.get("name", "")}
            for item in subs
            if isinstance(item, dict)
        ]
    return safe


def _collect_log_files(log_dir: Path, domains) -> list[Path]:
    found: list[Path] = []
    for domain in domains:
        name = _DOMAIN_LOG_FILES.get(domain)
        if not name:
            continue
        base = log_dir / name
        if base.exists():
            found.append(base)
        for index in range(1, _LOG_ROTATIONS + 1):
            rotated = log_dir / f"{name}.{index}"
            if rotated.exists():
                found.append(rotated)
    return found


def collect_network_context() -> dict[str, any]:
    """Gather diagnostic info about system's active network stack without blocking."""
    import socket
    
    has_ipv4_internet = False
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.settimeout(0.5)
        s.connect(("1.1.1.1", 80))
        has_ipv4_internet = True
        s.close()
    except Exception:
        pass

    has_ipv6_internet = False
    try:
        s = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
        s.settimeout(0.5)
        s.connect(("2001:4860:4860::8888", 80))
        has_ipv6_internet = True
        s.close()
    except Exception:
        pass

    dns_servers = []
    if platform.system() == "Windows":
        import winreg
        try:
            reg_path = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters"
            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, reg_path) as key:
                dhcp_dns, _ = winreg.QueryValueEx(key, "DhcpNameServer")
                if dhcp_dns:
                    dns_servers.extend(str(dhcp_dns).split())
        except OSError:
            pass
        try:
            reg_path = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters"
            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, reg_path) as key:
                dns, _ = winreg.QueryValueEx(key, "NameServer")
                if dns:
                    dns_servers.extend(str(dns).split())
        except OSError:
            pass

    unique_dns = []
    for ip in dns_servers:
        cleaned = ip.replace(",", " ").strip()
        for sub in cleaned.split():
            if sub not in unique_dns:
                unique_dns.append(sub)

    proxy_info = {
        "env_proxies": {},
        "windows_proxy_enable": None,
        "windows_proxy_server": None,
        "windows_auto_config_url": None,
    }
    try:
        import urllib.request
        proxy_info["env_proxies"] = urllib.request.getproxies()
    except Exception:
        pass

    if platform.system() == "Windows":
        import winreg
        try:
            reg_path = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, reg_path) as key:
                try:
                    enable, _ = winreg.QueryValueEx(key, "ProxyEnable")
                    proxy_info["windows_proxy_enable"] = bool(enable)
                except OSError:
                    pass
                try:
                    server, _ = winreg.QueryValueEx(key, "ProxyServer")
                    proxy_info["windows_proxy_server"] = str(server)
                except OSError:
                    pass
                try:
                    auto_url, _ = winreg.QueryValueEx(key, "AutoConfigURL")
                    proxy_info["windows_auto_config_url"] = str(auto_url)
                except OSError:
                    pass
        except OSError:
            pass

    connected_adapters = []
    if platform.system() == "Windows":
        import subprocess
        try:
            creation_flags = 0
            if hasattr(subprocess, "CREATE_NO_WINDOW"):
                creation_flags = subprocess.CREATE_NO_WINDOW
            res = subprocess.run(
                ["netsh", "interface", "show", "interface"],
                capture_output=True,
                timeout=2.0,
                creationflags=creation_flags
            )
            if res.returncode == 0:
                stdout_str = decode_output(res.stdout)

                for line in stdout_str.splitlines():
                    parts = line.split()
                    if len(parts) >= 4:
                        state_val = parts[1].lower()
                        if state_val in ("connected", "подключено"):
                            interface_name = " ".join(parts[3:])
                            connected_adapters.append(interface_name)
        except Exception:
            pass

    return {
        "ipv4_internet": has_ipv4_internet,
        "ipv6_internet": has_ipv6_internet,
        "system_dns": unique_dns,
        "proxy_info": proxy_info,
        "connected_adapters": connected_adapters,
    }


def export_diagnostics(
    zip_path: Path,
    state: AppState,
    logs: list[str],
    include: dict | None = None,
) -> Path:
    """Bundle the selected diagnostic sections into a zip.

    ``include`` is a mapping of section -> bool. Supported sections:
    ``errors``/``core``/``app``/``traffic`` (domain log files), ``state``
    (redacted app state) and ``recent`` (in-memory UI log tail). When
    ``include`` is falsy every section is bundled (legacy behaviour).
    """
    zip_path.parent.mkdir(parents=True, exist_ok=True)

    flags = _normalize_include(include)
    selected_domains = [d for d in _DOMAIN_LOG_FILES if flags.get(d)]
    log_files = _collect_log_files(LOG_DIR, selected_domains)

    meta = {
        "app_version": APP_VERSION,
        "install_id": get_install_id(),
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(),
        "python": platform.python_version(),
        "sections": [key for key, on in flags.items() if on],
        "log_files": [path.name for path in log_files],
        "network": collect_network_context(),
    }

    with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("meta.json", json.dumps(meta, ensure_ascii=True, indent=2))
        if flags.get("state"):
            safe_state = _sanitize_state(state.to_dict())
            archive.writestr(
                "state_redacted.json",
                json.dumps(safe_state, ensure_ascii=True, indent=2),
            )
        if flags.get("recent"):
            archive.writestr("recent_logs.txt", "\n".join(logs[-2000:]))
        for path in log_files:
            try:
                archive.write(path, arcname=f"logs/{path.name}")
            except OSError:
                pass
        fh_log = LOG_DIR / "faulthandler.log"
        if fh_log.is_file():
            try:
                archive.write(fh_log, arcname=f"logs/{fh_log.name}")
            except OSError:
                pass

    return zip_path
