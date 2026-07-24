from __future__ import annotations

import json
import hashlib
import os
import re
import shutil
import subprocess
import time
import ctypes
from dataclasses import dataclass
from pathlib import Path

from .constants import DATA_DIR, INSTALL_DATA_DIR
from .subprocess_utils import CREATE_NO_WINDOW, result_output_text, run_text_pumped


DROUTE_LEGACY_VERSION = "1.1.2"
DROUTE_SOURCE_URL = "https://github.com/snowluwu/droute"
DROUTE_BUNDLED_DIR = INSTALL_DATA_DIR / "external" / "droute"
DROUTE_DIR = DATA_DIR / "external" / "droute"
DROUTE_EXE = DROUTE_DIR / "droute.exe"
DROUTE_NOTICE = DROUTE_DIR / "README.droute.txt"
DROUTE_VERSION_FILE = DROUTE_DIR / "version.txt"
DROUTE_INSTALL_VERSION_FILE = "Lumen.droute.version"
LEGACY_DROUTE_INSTALL_VERSION_FILE = "LumenKVN.droute.version"

_DISCORD_BRANCHES = {
    "stable": ("Discord", "Discord.exe"),
    "canary": ("DiscordCanary", "DiscordCanary.exe"),
    "ptb": ("DiscordPTB", "DiscordPTB.exe"),
}


@dataclass(slots=True)
class DiscordInstall:
    branch_id: str
    root: Path
    app_dir: Path
    exe_path: Path
    process_name: str


@dataclass(slots=True)
class DiscordProxyResult:
    ok: bool
    message: str
    affected: int = 0


def _version_key(path: Path) -> tuple[int, ...]:
    raw = path.name.removeprefix("app-")
    parts: list[int] = []
    for chunk in raw.split("."):
        try:
            parts.append(int(chunk))
        except ValueError:
            parts.append(0)
    return tuple(parts)


def _local_appdata() -> Path:
    value = os.environ.get("LOCALAPPDATA")
    if value:
        return Path(value)
    return Path.home() / "AppData" / "Local"


def _find_install(branch_id: str) -> DiscordInstall | None:
    folder, exe_name = _DISCORD_BRANCHES[branch_id]
    root = _local_appdata() / folder
    if not root.is_dir():
        return None
    app_dirs = [p for p in root.glob("app-*") if p.is_dir()]
    if not app_dirs:
        return None
    app_dir = sorted(app_dirs, key=_version_key, reverse=True)[0]
    exe_path = app_dir / exe_name
    if not exe_path.is_file():
        return None
    return DiscordInstall(branch_id, root, app_dir, exe_path, exe_name)


def find_installed_discords() -> list[DiscordInstall]:
    installs: list[DiscordInstall] = []
    for branch_id in _DISCORD_BRANCHES:
        install = _find_install(branch_id)
        if install is not None:
            installs.append(install)
    return installs


def migrate_legacy_droute_markers() -> None:
    """Rename the old product-branded Discord marker without reinstalling droute."""
    for install in find_installed_discords():
        legacy = install.root / LEGACY_DROUTE_INSTALL_VERSION_FILE
        current = install.root / DROUTE_INSTALL_VERSION_FILE
        if not legacy.is_file():
            continue
        try:
            if current.is_file():
                legacy.unlink(missing_ok=True)
            else:
                os.replace(legacy, current)
        except OSError:
            continue


def _write_droute_notice() -> None:
    DROUTE_NOTICE.write_text(
        "droute is a bundled external GPL-3.0 component originally published at:\n"
        f"{DROUTE_SOURCE_URL}\n\n"
        "Lumen includes the frozen droute 2.0.0 binary distribution because the\n"
        "upstream repository is no longer available. The external binary is used\n"
        "to install a Discord-local version.dll loader, droute.dll payload and Squirrel\n"
        "updater hook for Discord TCP/UDP SOCKS5 proxying.\n",
        encoding="utf-8",
    )


def _droute_version_at(directory: Path) -> str:
    exe = directory / DROUTE_EXE.name
    if not exe.is_file() or exe.stat().st_size < 1024:
        return ""
    try:
        version = (directory / DROUTE_VERSION_FILE.name).read_text(
            encoding="utf-8"
        ).strip().lstrip("v")
    except OSError:
        version = ""
    return version or DROUTE_LEGACY_VERSION


def _verify_bundled_droute(directory: Path) -> None:
    manifest = directory / "SHA256SUMS.txt"
    try:
        lines = manifest.read_text(encoding="utf-8").splitlines()
    except OSError as exc:
        raise RuntimeError("У встроенного droute отсутствует список контрольных сумм.") from exc
    for line in lines:
        expected, separator, relative_name = line.strip().partition("  ")
        if not separator or not re.fullmatch(r"[0-9a-f]{64}", expected):
            raise RuntimeError("Список контрольных сумм droute поврежден.")
        source = (directory / relative_name).resolve()
        try:
            source.relative_to(directory.resolve())
        except ValueError as exc:
            raise RuntimeError("Некорректный путь в списке контрольных сумм droute.") from exc
        if not source.is_file() or hashlib.sha256(source.read_bytes()).hexdigest() != expected:
            raise RuntimeError(f"Контрольная сумма droute не совпадает: {relative_name}")


def _current_droute_version() -> str:
    if not DROUTE_EXE.is_file() or DROUTE_EXE.stat().st_size <= 0:
        return ""
    try:
        version = DROUTE_VERSION_FILE.read_text(encoding="utf-8").strip().lstrip("v")
    except OSError:
        version = ""
    return version or DROUTE_LEGACY_VERSION


def _droute_version_key(value: str) -> tuple[int, ...]:
    return tuple(int(part) for part in re.findall(r"\d+", value))


def get_bundled_droute_version(bundle_dir: Path | None = None) -> str:
    directory = bundle_dir or DROUTE_BUNDLED_DIR
    version = _droute_version_at(directory)
    if not version:
        return ""
    try:
        _verify_bundled_droute(directory)
    except RuntimeError:
        return ""
    return version


def install_bundled_droute(
    target_dir: Path | None = None,
    *,
    bundle_dir: Path | None = None,
) -> str:
    source = bundle_dir or DROUTE_BUNDLED_DIR
    target = target_dir or DROUTE_DIR
    version = _droute_version_at(source)
    if not version:
        raise RuntimeError("Встроенный droute не найден. Переустановите Lumen.")
    _verify_bundled_droute(source)
    try:
        if source.resolve() == target.resolve():
            return version
    except OSError:
        pass

    target.mkdir(parents=True, exist_ok=True)
    for source_file in source.rglob("*"):
        if not source_file.is_file():
            continue
        destination = target / source_file.relative_to(source)
        destination.parent.mkdir(parents=True, exist_ok=True)
        staged = destination.with_name(destination.name + ".lumen-new")
        try:
            shutil.copy2(source_file, staged)
            os.replace(staged, destination)
        finally:
            staged.unlink(missing_ok=True)
    if _droute_version_at(target) != version:
        raise RuntimeError("Не удалось развернуть встроенный droute.")
    return version


def get_droute_bundle_version() -> str:
    bundled_version = get_bundled_droute_version()
    current_version = _current_droute_version()
    if bundled_version and (
        not current_version
        or _droute_version_key(bundled_version) > _droute_version_key(current_version)
    ):
        try:
            install_bundled_droute()
        except OSError:
            pass
        current_version = _current_droute_version()
    return current_version


def ensure_droute_bundle() -> Path:
    DROUTE_DIR.mkdir(parents=True, exist_ok=True)
    bundled_version = get_bundled_droute_version()
    current_version = _current_droute_version()
    if bundled_version and (
        not current_version
        or _droute_version_key(bundled_version) > _droute_version_key(current_version)
    ):
        install_bundled_droute()
    if DROUTE_EXE.is_file() and DROUTE_EXE.stat().st_size > 1024:
        _write_droute_notice()
        return DROUTE_EXE
    raise RuntimeError("Встроенный droute не найден. Переустановите Lumen.")


def _powershell_quote(value: str | Path) -> str:
    return "'" + str(value).replace("'", "''") + "'"


def _run_powershell(script: str, *, timeout: float = 20.0) -> subprocess.CompletedProcess[bytes]:
    return run_text_pumped(
        ["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
        timeout=timeout,
        creationflags=CREATE_NO_WINDOW,
    )


def _discord_processes() -> list[dict[str, object]]:
    native = _discord_processes_win32()
    if native is not None:
        return native
    names = ",".join(f"'{exe}'" for _, exe in _DISCORD_BRANCHES.values())
    script = (
        f"$names = @({names}); "
        "$items = @(Get-CimInstance Win32_Process | Where-Object { $names -contains $_.Name } | "
        "Select-Object ProcessId,Name,ExecutablePath); "
        "if ($items.Count -eq 0) { '[]' } else { $items | ConvertTo-Json -Compress }"
    )
    result = _run_powershell(script, timeout=6)
    if result.returncode != 0:
        return []
    text = result_output_text(result).strip()
    if not text:
        return []
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        return []
    if isinstance(data, dict):
        return [data]
    if isinstance(data, list):
        return [item for item in data if isinstance(item, dict)]
    return []


def _discord_processes_win32() -> list[dict[str, object]] | None:
    if os.name != "nt":
        return None
    wanted = {exe.lower() for _, exe in _DISCORD_BRANCHES.values()}

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
        kernel32.OpenProcess.argtypes = (ctypes.c_uint32, ctypes.c_bool, ctypes.c_uint32)
        kernel32.OpenProcess.restype = ctypes.c_void_p
        kernel32.QueryFullProcessImageNameW.argtypes = (
            ctypes.c_void_p,
            ctypes.c_uint32,
            ctypes.c_wchar_p,
            ctypes.POINTER(ctypes.c_uint32),
        )
        kernel32.QueryFullProcessImageNameW.restype = ctypes.c_bool
        kernel32.CloseHandle.argtypes = (ctypes.c_void_p,)
        snapshot = kernel32.CreateToolhelp32Snapshot(0x00000002, 0)
        if not snapshot or snapshot == ctypes.c_void_p(-1).value:
            return None
        try:
            entry = PROCESSENTRY32W()
            entry.dwSize = ctypes.sizeof(PROCESSENTRY32W)
            if not kernel32.Process32FirstW(snapshot, ctypes.byref(entry)):
                return []
            result: list[dict[str, object]] = []
            while True:
                name = str(entry.szExeFile)
                if name.lower() in wanted:
                    pid = int(entry.th32ProcessID)
                    result.append(
                        {
                            "ProcessId": pid,
                            "Name": name,
                            "ExecutablePath": _query_process_path_win32(kernel32, pid),
                        }
                    )
                if not kernel32.Process32NextW(snapshot, ctypes.byref(entry)):
                    break
            return result
        finally:
            kernel32.CloseHandle(snapshot)
    except Exception:
        return None


def _query_process_path_win32(kernel32, pid: int) -> str:
    PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
    handle = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, int(pid))
    if not handle:
        return ""
    try:
        size = ctypes.c_uint32(32768)
        buffer = ctypes.create_unicode_buffer(size.value)
        if not kernel32.QueryFullProcessImageNameW(handle, 0, buffer, ctypes.byref(size)):
            return ""
        return str(buffer.value)
    finally:
        kernel32.CloseHandle(handle)


def _terminate_discord(install: DiscordInstall) -> None:
    try:
        run_text_pumped(
            ["taskkill", "/F", "/T", "/IM", install.process_name],
            timeout=8,
            creationflags=CREATE_NO_WINDOW,
        )
    except Exception:
        pass


def _launch_discord(install: DiscordInstall) -> None:
    subprocess.Popen(
        [str(install.exe_path)],
        cwd=str(install.app_dir),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        close_fds=True,
    )


def _process_pids_for_install(install: DiscordInstall) -> list[int]:
    pids: list[int] = []
    install_root = str(install.root).lower()
    for item in _discord_processes():
        if str(item.get("Name") or "").lower() != install.process_name.lower():
            continue
        exe_path = str(item.get("ExecutablePath") or "").lower()
        if install_root not in exe_path:
            continue
        try:
            pid = int(item.get("ProcessId") or 0)
        except (TypeError, ValueError):
            continue
        if pid > 0:
            pids.append(pid)
    return sorted(set(pids))


def _wait_for_discord_start(install: DiscordInstall, timeout_sec: float = 10.0) -> bool:
    deadline = time.monotonic() + timeout_sec
    while time.monotonic() < deadline:
        if _process_pids_for_install(install):
            return True
        time.sleep(0.25)
    return False


def _wait_for_discord_exit(install: DiscordInstall, timeout_sec: float = 8.0) -> bool:
    deadline = time.monotonic() + timeout_sec
    while time.monotonic() < deadline:
        if not _process_pids_for_install(install):
            return True
        time.sleep(0.25)
    return False


def _write_droute_registry(socks_port: int) -> None:
    script = (
        "$path = 'HKCU:\\Software\\droute'; "
        "New-Item -Path $path -Force | Out-Null; "
        "Set-ItemProperty -Path $path -Name Host -Type String -Value '127.0.0.1'; "
        f"Set-ItemProperty -Path $path -Name Port -Type DWord -Value {int(socks_port)}; "
        "Set-ItemProperty -Path $path -Name User -Type String -Value ''; "
        "Set-ItemProperty -Path $path -Name Password -Type String -Value ''; "
        "Set-ItemProperty -Path $path -Name ConnectTimeout -Type DWord -Value 5000; "
        "Set-ItemProperty -Path $path -Name ReconnectInterval -Type DWord -Value 3000; "
        "Set-ItemProperty -Path $path -Name RetryTimeout -Type DWord -Value 10000; "
        "Set-ItemProperty -Path $path -Name LogLevel -Type DWord -Value 2"
    )
    result = _run_powershell(script, timeout=8)
    if result.returncode != 0:
        raise RuntimeError(result_output_text(result) or "failed to write droute registry settings")


def _read_droute_registry_port() -> int:
    if os.name != "nt":
        return 0
    try:
        import winreg

        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\droute", 0, winreg.KEY_READ) as key:
            value, _ = winreg.QueryValueEx(key, "Port")
        return int(value)
    except Exception:
        return 0


def _droute_payload_installed(install: DiscordInstall) -> bool:
    files_installed = (
        (install.app_dir / "version.dll").is_file()
        and (install.app_dir / "droute.dll").is_file()
        and (install.root / "Droute.UpdaterHook.dll").is_file()
        and (install.root / "Update.exe.config").is_file()
    )
    if not files_installed:
        return False
    current_version = get_droute_bundle_version()
    if not current_version:
        return False
    try:
        marker = install.root / DROUTE_INSTALL_VERSION_FILE
        if not marker.is_file():
            legacy_marker = install.root / LEGACY_DROUTE_INSTALL_VERSION_FILE
            installed_version = legacy_marker.read_text(encoding="utf-8").strip()
            marker.write_text(installed_version + "\n", encoding="utf-8")
            legacy_marker.unlink(missing_ok=True)
        else:
            installed_version = marker.read_text(encoding="utf-8").strip()
            (install.root / LEGACY_DROUTE_INSTALL_VERSION_FILE).unlink(missing_ok=True)
    except OSError:
        # Existing droute installations predate version markers. Keep them as-is
        # until the user explicitly downloads an update, which creates version.txt.
        return not DROUTE_VERSION_FILE.is_file()
    return installed_version == current_version


def _droute_payload_present(install: DiscordInstall) -> bool:
    # any leftover droute file counts, partial installs must be cleaned too
    if (
        (install.root / "Droute.UpdaterHook.dll").is_file()
        or (install.root / "Update.exe.config").is_file()
        or (install.root / DROUTE_INSTALL_VERSION_FILE).is_file()
        or (install.root / LEGACY_DROUTE_INSTALL_VERSION_FILE).is_file()
    ):
        return True
    for app_dir in install.root.glob("app-*"):
        if app_dir.is_dir() and any((app_dir / name).is_file() for name in ("version.dll", "droute.dll")):
            return True
    return False


def _install_droute_payload(exe: Path, install: DiscordInstall) -> None:
    app_dir = install.app_dir
    branch_root = install.root
    proxy_path = app_dir / "version.dll"
    payload_path = app_dir / "droute.dll"
    updater_hook_path = branch_root / "Droute.UpdaterHook.dll"
    updater_config_path = branch_root / "Update.exe.config"

    script = (
        "$ErrorActionPreference = 'Stop'; "
        f"$asm = [System.Reflection.Assembly]::LoadFile({_powershell_quote(exe)}); "
        "$patchType = $asm.GetType('Droute.Core.PatchManager', $true); "
        "$archType = $patchType.GetNestedType('ArchitectureBitness'); "
        "$force64 = [Enum]::Parse($archType, 'Force64'); "
        "$dup = $patchType.GetMethod('DuplicateProxy', [Type[]]@([string], $archType)); "
        "$apply = $patchType.GetMethod('ApplyPEPatch', [Type[]]@([string])); "
        f"$dup.Invoke($null, @({_powershell_quote(proxy_path)}, $force64)); "
        f"$apply.Invoke($null, @({_powershell_quote(proxy_path)})); "
        "$stream = $asm.GetManifestResourceStream('Droute.Installer.Properties.Resources.resources'); "
        "$reader = New-Object System.Resources.ResourceReader($stream); "
        "$items = @{}; foreach ($entry in $reader) { $items[$entry.Key] = $entry.Value }; "
        f"[IO.File]::WriteAllBytes({_powershell_quote(payload_path)}, [byte[]]$items['Droute64']); "
        f"[IO.File]::WriteAllBytes({_powershell_quote(updater_hook_path)}, [byte[]]$items['UpdaterHook']); "
        f"[IO.File]::WriteAllText({_powershell_quote(updater_config_path)}, [string]$items['UpdaterConfig']); "
        "$reader.Close(); $stream.Close()"
    )
    result = _run_powershell(script, timeout=30)
    if result.returncode != 0:
        raise RuntimeError(result_output_text(result) or f"failed to install droute for {install.process_name}")
    version = get_droute_bundle_version()
    if not version:
        raise RuntimeError("droute bundle version is unknown")
    (branch_root / DROUTE_INSTALL_VERSION_FILE).write_text(version + "\n", encoding="utf-8")
    (branch_root / LEGACY_DROUTE_INSTALL_VERSION_FILE).unlink(missing_ok=True)


def _remove_droute_payload(install: DiscordInstall) -> list[str]:
    targets = [
        install.root / "Droute.UpdaterHook.dll",
        install.root / "Update.exe.config",
        install.root / DROUTE_INSTALL_VERSION_FILE,
        install.root / LEGACY_DROUTE_INSTALL_VERSION_FILE,
    ]
    for app_dir in install.root.glob("app-*"):
        if app_dir.is_dir():
            targets.extend(app_dir / name for name in ("version.dll", "droute.dll"))
    leftovers: list[str] = []
    for path in targets:
        removed = False
        for _attempt in range(12):  # loader dll handle can outlive taskkill for a moment
            try:
                path.unlink(missing_ok=True)
                removed = True
                break
            except PermissionError:
                time.sleep(0.25)
        if not removed and path.is_file():
            leftovers.append(path.name)
    return sorted(set(leftovers))


class DiscordProxyManager:
    def enable(self, socks_port: int) -> DiscordProxyResult:
        if os.name != "nt":
            return DiscordProxyResult(False, "Discord proxy is only available on Windows")
        installs = find_installed_discords()
        if not installs:
            return DiscordProxyResult(False, "Discord not found")
        if int(socks_port) <= 0:
            return DiscordProxyResult(False, "Lumen SOCKS5 port not found")

        target_port = int(socks_port)
        try:
            needs_install = any(not _droute_payload_installed(install) for install in installs)
            exe = ensure_droute_bundle() if needs_install else DROUTE_EXE
            if not exe.is_file():
                exe = ensure_droute_bundle()
            current_port = _read_droute_registry_port()
            if current_port != target_port:
                _write_droute_registry(target_port)
        except Exception as exc:
            return DiscordProxyResult(False, f"Failed to prepare droute: {exc}")

        affected = 0
        already_ready = 0
        errors: list[str] = []
        for install in installs:
            try:
                if _droute_payload_installed(install):
                    already_ready += 1
                    affected += 1
                    continue
                was_running = bool(_process_pids_for_install(install))
                _terminate_discord(install)
                _install_droute_payload(exe, install)
                if was_running:
                    _launch_discord(install)
                    _wait_for_discord_start(install)
                affected += 1
            except Exception as exc:
                errors.append(f"{install.process_name}: {exc}")

        if affected:
            message = f"droute ready for Discord via SOCKS5 127.0.0.1:{target_port}"
            if already_ready and already_ready == affected:
                message = f"droute already active for Discord via SOCKS5 127.0.0.1:{target_port}"
            if errors:
                message += f"; partial errors: {'; '.join(errors[:2])}"
            return DiscordProxyResult(True, message, affected)
        return DiscordProxyResult(False, "; ".join(errors) or "Failed to enable Discord proxy")

    def disable(self) -> DiscordProxyResult:
        installs = find_installed_discords()
        if not installs:
            return DiscordProxyResult(False, "Discord not found")
        affected = 0
        errors: list[str] = []
        for install in installs:
            try:
                if not _droute_payload_present(install):
                    continue  # nothing to remove, skip the needless Discord restart
                was_running = bool(_process_pids_for_install(install))
                _terminate_discord(install)
                if was_running:
                    _wait_for_discord_exit(install)  # version.dll stays locked until the process really exits
                leftovers = _remove_droute_payload(install)
                if was_running:
                    _launch_discord(install)
                    _wait_for_discord_start(install)
                if leftovers:
                    errors.append(f"{install.process_name}: droute files still locked: {', '.join(leftovers)}")
                else:
                    affected += 1
            except Exception as exc:
                errors.append(f"{install.process_name}: {exc}")
        if errors:
            return DiscordProxyResult(False, "; ".join(errors))
        if affected:
            return DiscordProxyResult(True, "droute removed from Discord", affected)
        return DiscordProxyResult(True, "droute is not installed in Discord", 0)
