from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys

if sys.platform == "win32":
    import ctypes
    import winreg


RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
STARTUP_APPROVED_RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
APP_COMPAT_LAYERS_KEY = r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers"
RUNASADMIN_FLAG = "RUNASADMIN"
TASK_NAME = "Lumen KVN"
CREATE_NO_WINDOW = 0x08000000
STARTUP_STATE_ABSENT = "absent"
STARTUP_STATE_ENABLED = "enabled"
STARTUP_STATE_DISABLED = "disabled"


def set_startup_enabled(app_name: str, enabled: bool, command: str) -> None:
    if sys.platform != "win32":
        return
    _delete_startup_task()
    if enabled:
        _create_registry_startup(app_name, command)
        _set_startup_approved(app_name, enabled=True)
    else:
        _delete_registry_startup(app_name)
        _delete_startup_approved(app_name)


def get_startup_state(app_name: str) -> str:
    if sys.platform != "win32":
        return STARTUP_STATE_ABSENT
    if not _registry_startup_exists(app_name):
        return STARTUP_STATE_ABSENT
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, STARTUP_APPROVED_RUN_KEY, 0, winreg.KEY_READ) as key:
            value, _ = winreg.QueryValueEx(key, app_name)
    except FileNotFoundError:
        return STARTUP_STATE_ENABLED
    if isinstance(value, (bytes, bytearray)) and value and value[0] == 0x03:
        return STARTUP_STATE_DISABLED
    return STARTUP_STATE_ENABLED


def is_process_elevated() -> bool:
    if sys.platform != "win32":
        return True
    try:
        return bool(ctypes.windll.shell32.IsUserAnAdmin())
    except Exception:
        return False


def relaunch_as_admin(extra_args: list[str] | None = None) -> bool:
    if sys.platform != "win32":
        return False
    executable, arguments, working_dir = _admin_launch_command(extra_args)
    try:
        result = ctypes.windll.shell32.ShellExecuteW(
            None,
            "runas",
            str(executable),
            arguments,
            str(working_dir),
            1,
        )
        return int(result) > 32
    except Exception:
        return False


def set_always_run_as_admin(enabled: bool, executable: str | Path | None = None) -> None:
    if sys.platform != "win32":
        return
    target = str(Path(executable).resolve() if executable else _current_executable_path())
    with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER, APP_COMPAT_LAYERS_KEY, 0, winreg.KEY_READ | winreg.KEY_WRITE) as key:
        current = ""
        try:
            current, _ = winreg.QueryValueEx(key, target)
        except FileNotFoundError:
            current = ""
        updated = _update_layer_value(str(current or ""), enabled)
        if updated:
            winreg.SetValueEx(key, target, 0, winreg.REG_SZ, updated)
        else:
            try:
                winreg.DeleteValue(key, target)
            except FileNotFoundError:
                pass


def is_always_run_as_admin_enabled(executable: str | Path | None = None) -> bool:
    if sys.platform != "win32":
        return False
    target = str(Path(executable).resolve() if executable else _current_executable_path())
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, APP_COMPAT_LAYERS_KEY, 0, winreg.KEY_READ) as key:
            value, _ = winreg.QueryValueEx(key, target)
    except FileNotFoundError:
        return False
    return RUNASADMIN_FLAG in {part.upper() for part in str(value or "").split()}


def _current_executable_path() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve()
    return Path(sys.executable).resolve()


def _admin_launch_command(extra_args: list[str] | None = None) -> tuple[Path, str, Path]:
    args = [arg for arg in sys.argv[1:] if arg != "--relaunch-as-admin"]
    if extra_args:
        args.extend(extra_args)
    args.append("--relaunch-as-admin")
    if getattr(sys, "frozen", False):
        executable = Path(sys.executable).resolve()
        return executable, subprocess.list2cmdline(args), executable.parent

    base_dir = Path(__file__).resolve().parents[1]
    pythonw = base_dir / ".venv" / "Scripts" / "pythonw.exe"
    executable = pythonw if pythonw.exists() else Path(sys.executable).resolve()
    script = base_dir / "run_qml.py"
    return executable, subprocess.list2cmdline([str(script), *args]), base_dir


def _update_layer_value(value: str, enabled: bool) -> str:
    parts = [part for part in str(value or "").split() if part and part != "~"]
    flags = {part.upper(): part for part in parts}
    if enabled:
        flags[RUNASADMIN_FLAG] = RUNASADMIN_FLAG
    else:
        flags.pop(RUNASADMIN_FLAG, None)
    if not flags:
        return ""
    return "~ " + " ".join(flags.values())


def _delete_registry_startup(app_name: str) -> None:
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
            try:
                winreg.DeleteValue(key, app_name)
            except FileNotFoundError:
                pass
    except FileNotFoundError:
        pass


def _create_registry_startup(app_name: str, command: str) -> None:
    with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
        winreg.SetValueEx(key, app_name, 0, winreg.REG_SZ, command)


def _registry_startup_exists(app_name: str) -> bool:
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_READ) as key:
            winreg.QueryValueEx(key, app_name)
        return True
    except FileNotFoundError:
        return False


def _set_startup_approved(app_name: str, *, enabled: bool) -> None:
    # 0x02 = enabled, 0x03 = disabled; the remaining bytes are FILETIME-ish
    # metadata and can be zero for entries created by the app itself.
    payload = bytes([0x02 if enabled else 0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
    with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER, STARTUP_APPROVED_RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
        winreg.SetValueEx(key, app_name, 0, winreg.REG_BINARY, payload)


def _delete_startup_approved(app_name: str) -> None:
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, STARTUP_APPROVED_RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
            try:
                winreg.DeleteValue(key, app_name)
            except FileNotFoundError:
                pass
    except FileNotFoundError:
        pass


def _create_startup_task(command: str) -> None:
    _delete_startup_task()
    subprocess.run(
        [
            "schtasks",
            "/Create",
            "/TN",
            TASK_NAME,
            "/SC",
            "ONLOGON",
            "/RL",
            "HIGHEST",
            "/RU",
            _current_task_user(),
            "/TR",
            command,
            "/F",
        ],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        creationflags=CREATE_NO_WINDOW,
    )


def _delete_startup_task() -> None:
    subprocess.run(
        ["schtasks", "/Delete", "/TN", TASK_NAME, "/F"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        creationflags=CREATE_NO_WINDOW,
    )


def _current_task_user() -> str:
    domain = os.environ.get("USERDOMAIN", "").strip()
    user = os.environ.get("USERNAME", "").strip()
    if domain and user:
        return f"{domain}\\{user}"
    return user or os.getlogin()


def _installed_executable_path() -> Path | None:
    candidates: list[Path] = []
    for env_name in ("ProgramFiles", "ProgramFiles(x86)"):
        root = os.environ.get(env_name, "").strip()
        if root:
            candidates.append(Path(root) / "Lumen KVN" / "LumenKVN.exe")
    candidates.append(Path(r"C:\Program Files\Lumen KVN\LumenKVN.exe"))
    candidates.append(Path(r"C:\Program Files (x86)\Lumen KVN\LumenKVN.exe"))
    for candidate in candidates:
        try:
            if candidate.is_file():
                return candidate.resolve()
        except OSError:
            continue
    return None


def build_startup_command(*, in_tray: bool = True) -> str:
    tray_args = ["--tray"] if in_tray else []
    if getattr(sys, "frozen", False):
        exe = Path(sys.executable).resolve()
        return subprocess.list2cmdline([str(exe), *tray_args])

    installed_exe = _installed_executable_path()
    if installed_exe is not None:
        return subprocess.list2cmdline([str(installed_exe), *tray_args])

    base_dir = Path(__file__).resolve().parents[1]
    script = base_dir / "run_qml.py"
    venv_pythonw = base_dir / ".venv" / "Scripts" / "pythonw.exe"
    python_exe = venv_pythonw if venv_pythonw.exists() else Path(sys.executable).resolve()
    return subprocess.list2cmdline([str(python_exe), str(script), *tray_args])
