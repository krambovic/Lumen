from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys

if sys.platform == "win32":
    import winreg


RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
TASK_NAME = "Bebra VPN"
CREATE_NO_WINDOW = 0x08000000


def set_startup_enabled(app_name: str, enabled: bool, command: str) -> None:
    if sys.platform != "win32":
        return
    _delete_registry_startup(app_name)
    if enabled:
        _create_startup_task(command)
    else:
        _delete_startup_task()


def _delete_registry_startup(app_name: str) -> None:
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
        try:
            winreg.DeleteValue(key, app_name)
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
        creationflags=CREATE_NO_WINDOW,
    )


def _delete_startup_task() -> None:
    subprocess.run(
        ["schtasks", "/Delete", "/TN", TASK_NAME, "/F"],
        capture_output=True,
        text=True,
        creationflags=CREATE_NO_WINDOW,
    )


def _current_task_user() -> str:
    domain = os.environ.get("USERDOMAIN", "").strip()
    user = os.environ.get("USERNAME", "").strip()
    if domain and user:
        return f"{domain}\\{user}"
    return user or os.getlogin()


def build_startup_command() -> str:
    if getattr(sys, "frozen", False):
        exe = Path(sys.executable).resolve()
        return f'"{exe}" --tray'

    base_dir = Path(__file__).resolve().parents[1]
    script = base_dir / "main.py"
    venv_pythonw = base_dir / ".venv" / "Scripts" / "pythonw.exe"
    python_exe = venv_pythonw if venv_pythonw.exists() else Path(sys.executable).resolve()
    return f'"{python_exe}" "{script}" --tray'
