"""Top-level launcher for the Qt Quick (QML) frontend.

This exists so the app can be started both as a module and as a PyInstaller
entry point. PyInstaller executes the entry script as ``__main__`` (not as a
package submodule), which would break the relative imports inside
``xray_fluent.qml_app.main_qml``. Using absolute imports here avoids that.

Dev usage:
    python run_qml.py
    # or
    python -m xray_fluent.qml_app.main_qml
"""
from __future__ import annotations

import os
import re
import shutil
import sys
import tempfile
import time
import traceback
from pathlib import Path


SUBSCRIPTION_FETCHER_EXE_NAME = "lumen-subscription-fetcher.exe"
_VERSION_TEXT_RE = re.compile(r"^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")


def _startup_log_path() -> Path:
    """Return a writable log path without importing the application package."""
    if getattr(sys, "frozen", False):
        base_dir = Path(sys.executable).resolve().parent
        if any((base_dir / marker).is_file() for marker in ("portable", "portable.txt")):
            candidates = (base_dir / "data" / "logs",)
        else:
            local_app_data = os.environ.get("LOCALAPPDATA", "").strip()
            candidates = tuple(
                directory
                for directory in (
                    Path(local_app_data) / "Lumen" / "data" / "logs"
                    if local_app_data
                    else None,
                    base_dir / "data" / "logs",
                )
                if directory is not None
            )
    else:
        candidates = (Path(__file__).resolve().parent / "data" / "logs",)

    for directory in candidates:
        try:
            directory.mkdir(parents=True, exist_ok=True)
            return directory / "startup-crash.log"
        except Exception:
            continue
    return Path(os.environ.get("TEMP", ".")) / "Lumen-startup-crash.log"


def _report_startup_failure(exc: BaseException, *, show_dialog: bool = True) -> None:
    log_path = _startup_log_path()
    try:
        log_path.write_text(
            "".join(traceback.format_exception(type(exc), exc, exc.__traceback__)),
            encoding="utf-8",
        )
    except Exception:
        pass

    if sys.platform != "win32" or not show_dialog:
        return
    try:
        import ctypes

        ctypes.windll.user32.MessageBoxW(
            None,
            (
                "Lumen не смог запуститься. Подробности сохранены в файле:\n\n"
                f"{log_path}"
            ),
            "Ошибка запуска Lumen",
            0x10,
        )
    except Exception:
        pass


def _is_subscription_fetcher() -> bool:
    return (
        "--subscription-fetcher" in sys.argv[1:]
        or Path(sys.executable).name.casefold() == SUBSCRIPTION_FETCHER_EXE_NAME.casefold()
    )


def _argument_value(name: str) -> str | None:
    try:
        index = sys.argv.index(name)
        return sys.argv[index + 1]
    except (ValueError, IndexError):
        return None


def _quarantine_misplaced_version_file(app_version: str) -> None:
    """Recover the root-level file created by the pre-fix updater.

    The old PowerShell version probe split paths at the first space and could
    write the version to ``C:\\Program``.  Only quarantine an exact semver
    payload matching the running application; arbitrary user files at the
    drive root must never be touched.
    """
    if sys.platform != "win32":
        return
    try:
        drive_root = Path(Path(sys.executable).resolve().anchor or r"C:\\")
        misplaced = drive_root / "Program"
        if not misplaced.is_file() or misplaced.stat().st_size > 128:
            return
        value = misplaced.read_text(encoding="utf-8-sig").strip()
        if not _VERSION_TEXT_RE.fullmatch(value):
            return
        if value.lstrip("v") != str(app_version).strip().lstrip("v"):
            return
        recovery_dir = Path(tempfile.gettempdir()) / "Lumen-recovered"
        recovery_dir.mkdir(parents=True, exist_ok=True)
        destination = recovery_dir / f"Program-{os.getpid()}-{time.time_ns()}.txt"
        shutil.move(str(misplaced), str(destination))
    except Exception:
        # Recovery is best-effort and must never prevent normal startup.
        pass


def _run() -> int:
    from xray_fluent.constants import APP_VERSION

    if _is_subscription_fetcher():
        from xray_fluent.subscription_fetcher import cli_main

        return int(cli_main())
    _quarantine_misplaced_version_file(APP_VERSION)
    if "--version-file" in sys.argv[1:]:
        try:
            output = _argument_value("--version-file")
            if not output:
                return 2
            target = Path(output).expanduser()
            if not target.is_absolute() or target.parent == Path(target.anchor):
                return 2
            target.write_text(APP_VERSION, encoding="utf-8")
            return 0
        except Exception:
            return 2
    if "--version" in sys.argv[1:]:
        print(APP_VERSION)
        return 0
    if "--startup-probe-file" in sys.argv[1:]:
        try:
            output = _argument_value("--startup-probe-file")
            if not output:
                return 2
            from PyQt6.QtQml import QQmlApplicationEngine  # noqa: F401
            from xray_fluent.qml_app.bridge import AppBridge  # noqa: F401
            from xray_fluent.qml_app.main_qml import main  # noqa: F401

            Path(output).write_text(APP_VERSION, encoding="utf-8")
            return 0
        except Exception:
            raise
    from xray_fluent.qml_app.main_qml import main

    return int(main())


if __name__ == "__main__":
    try:
        raise SystemExit(_run())
    except SystemExit:
        raise
    except BaseException as exc:
        _report_startup_failure(
            exc,
            show_dialog="--startup-probe-file" not in sys.argv[1:],
        )
        raise SystemExit(1)
