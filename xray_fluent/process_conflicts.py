from __future__ import annotations

import csv
import ctypes
import os
import re
import subprocess
from dataclasses import dataclass
from urllib.parse import urlsplit


_CREATE_NO_WINDOW = 0x08000000 if os.name == "nt" else 0
_KNOWN_CONFLICTS = {
    "amneziawg.exe": "AmneziaWG",
    "amneziavpn.exe": "AmneziaVPN",
    "clash-verge.exe": "Clash Verge",
    "clash-verge-rev.exe": "Clash Verge Rev",
    "clash.exe": "Clash",
    "flclash.exe": "FlClash",
    "happ-desktop.exe": "Happ",
    "happ.exe": "Happ",
    "hiddify.exe": "Hiddify",
    "karing.exe": "Karing",
    "mihomo.exe": "Mihomo",
    "nekobox.exe": "NekoBox",
    "nekoray.exe": "NekoRay",
    "nordvpn.exe": "NordVPN",
    "openvpn.exe": "OpenVPN",
    "outline-client.exe": "Outline",
    "protonvpn.exe": "Proton VPN",
    "psiphon3.exe": "Psiphon",
    "throne.exe": "Throne",
    "v2rayn.exe": "v2rayN",
    "wireguard.exe": "WireGuard",
}


@dataclass(frozen=True, slots=True)
class PortConflict:
    port: int
    pid: int
    process_name: str


def _running_processes() -> dict[int, str]:
    if os.name != "nt":
        return {}
    native = _running_processes_win32()
    if native is not None:
        return native
    try:
        result = subprocess.run(
            ["tasklist", "/FO", "CSV", "/NH"],
            capture_output=True,
            timeout=4,
            check=False,
            creationflags=_CREATE_NO_WINDOW,
        )
        text = result.stdout.decode("utf-8", errors="replace")
    except Exception:
        return {}
    processes: dict[int, str] = {}
    for row in csv.reader(text.splitlines()):
        if len(row) < 2:
            continue
        try:
            pid = int(row[1].replace("\u00a0", "").replace(" ", ""))
        except ValueError:
            continue
        processes[pid] = row[0].strip()
    return processes


def _running_processes_win32() -> dict[int, str] | None:
    """Enumerate process names through Toolhelp without spawning tasklist."""
    if os.name != "nt":
        return None

    class PROCESSENTRY32W(ctypes.Structure):
        _fields_ = [
            ("dwSize", ctypes.c_ulong),
            ("cntUsage", ctypes.c_ulong),
            ("th32ProcessID", ctypes.c_ulong),
            ("th32DefaultHeapID", ctypes.c_void_p),
            ("th32ModuleID", ctypes.c_ulong),
            ("cntThreads", ctypes.c_ulong),
            ("th32ParentProcessID", ctypes.c_ulong),
            ("pcPriClassBase", ctypes.c_long),
            ("dwFlags", ctypes.c_ulong),
            ("szExeFile", ctypes.c_wchar * 260),
        ]

    try:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.CreateToolhelp32Snapshot.argtypes = (ctypes.c_ulong, ctypes.c_ulong)
        kernel32.CreateToolhelp32Snapshot.restype = ctypes.c_void_p
        kernel32.Process32FirstW.argtypes = (ctypes.c_void_p, ctypes.POINTER(PROCESSENTRY32W))
        kernel32.Process32FirstW.restype = ctypes.c_bool
        kernel32.Process32NextW.argtypes = (ctypes.c_void_p, ctypes.POINTER(PROCESSENTRY32W))
        kernel32.Process32NextW.restype = ctypes.c_bool
        kernel32.CloseHandle.argtypes = (ctypes.c_void_p,)

        snapshot = kernel32.CreateToolhelp32Snapshot(0x00000002, 0)
        if not snapshot or snapshot == ctypes.c_void_p(-1).value:
            return None
        try:
            entry = PROCESSENTRY32W()
            entry.dwSize = ctypes.sizeof(PROCESSENTRY32W)
            if not kernel32.Process32FirstW(snapshot, ctypes.byref(entry)):
                return None
            processes: dict[int, str] = {}
            while True:
                processes[int(entry.th32ProcessID)] = str(entry.szExeFile)
                if not kernel32.Process32NextW(snapshot, ctypes.byref(entry)):
                    break
            return processes
        finally:
            kernel32.CloseHandle(snapshot)
    except Exception:
        return None


def find_conflicting_network_apps(processes: dict[int, str] | None = None) -> list[str]:
    processes = processes if processes is not None else _running_processes()
    found: set[str] = set()
    for process_name in processes.values():
        label = _KNOWN_CONFLICTS.get(process_name.strip().lower())
        if label:
            found.add(label)
    return sorted(found, key=str.casefold)


def is_process_name_running(process_name: str) -> bool:
    target = str(process_name or "").strip().lower()
    if not target:
        return False
    return any(name.strip().lower() == target for name in _running_processes().values())


def find_listening_port_conflicts(
    ports: set[int], *, ignored_pids: set[int] | None = None, processes: dict[int, str] | None = None
) -> list[PortConflict]:
    if os.name != "nt" or not ports:
        return []
    ignored = set(ignored_pids or ())
    processes = processes if processes is not None else _running_processes()
    try:
        result = subprocess.run(
            ["netstat", "-ano", "-p", "tcp"],
            capture_output=True,
            timeout=4,
            check=False,
            creationflags=_CREATE_NO_WINDOW,
        )
        text = result.stdout.decode("utf-8", errors="replace")
    except Exception:
        return []
    found: dict[int, PortConflict] = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) < 5 or parts[-2].upper() != "LISTENING":
            continue
        match = re.search(r":(\d+)$", parts[1])
        if not match:
            continue
        port = int(match.group(1))
        if port not in ports:
            continue
        try:
            pid = int(parts[-1])
        except ValueError:
            continue
        if pid in ignored:
            continue
        found[port] = PortConflict(port, pid, processes.get(pid, "неизвестный процесс"))
    return [found[port] for port in sorted(found)]


def _system_proxy_server() -> str:
    if os.name != "nt":
        return ""
    try:
        import winreg

        path = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path) as key:
            enabled = int(winreg.QueryValueEx(key, "ProxyEnable")[0] or 0)
            server = str(winreg.QueryValueEx(key, "ProxyServer")[0] or "").strip()
        return server if enabled and server else ""
    except (OSError, ValueError, TypeError):
        return ""


def _local_proxy_ports(proxy_server: str) -> set[int]:
    ports: set[int] = set()
    for chunk in re.split(r"[;\s]+", proxy_server):
        value = chunk.split("=", 1)[-1].strip()
        if not value:
            continue
        parsed = urlsplit(value if "://" in value else "//" + value)
        host = (parsed.hostname or "").lower()
        if host not in {"127.0.0.1", "localhost", "::1"}:
            continue
        try:
            if parsed.port:
                ports.add(int(parsed.port))
        except ValueError:
            continue
    return ports


def has_foreign_system_proxy(
    *, ignored_pids: set[int] | None = None, processes: dict[int, str] | None = None
) -> bool:
    server = _system_proxy_server()
    if not server:
        return False
    ports = _local_proxy_ports(server)
    if not ports:
        return True
    return bool(
        find_listening_port_conflicts(
            ports,
            ignored_pids=ignored_pids,
            processes=processes,
        )
    )


def scan_network_conflicts(ports: set[int], *, ignored_pids: set[int] | None = None) -> dict:
    processes = _running_processes()
    apps = find_conflicting_network_apps(processes)
    port_conflicts = find_listening_port_conflicts(
        ports, ignored_pids=ignored_pids, processes=processes
    )
    return {
        "apps": apps,
        "ports": port_conflicts,
        "unknown_client": bool(
            not apps
            and (
                port_conflicts
                or has_foreign_system_proxy(
                    ignored_pids=ignored_pids,
                    processes=processes,
                )
            )
        ),
    }
