"""Start the packaged Rust client under different per-process Windows setups.

Nothing system-wide is changed: each scenario extracts the portable build into
its own folder and launches it with its own environment (profile folders,
proxy variables, a pre-seeded state). The script checks that the window
appears, the process survives, the data folder is created where expected,
and that no panic is logged, then captures the client window and stops the
process it started.

    python rust/tools/env_matrix.py [--zip rust/dist/Lumen-portable-windows-x64.zip] [--work DIR]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "uicmp"))

import uicmp  # noqa: E402

LONG = "\\".join(["very-long-folder-name-for-path-limits"] * 4)


def scenarios(work: Path) -> list[dict]:
    plain_profile = {"settings": {}}
    return [
        {"name": "installed", "install": work / "plain" / "Lumen", "profile": work / "plain" / "profile",
         "portable": False},
        {"name": "portable", "install": work / "portable" / "Lumen", "profile": work / "portable" / "profile",
         "portable": True},
        {"name": "cyrillic-spaces", "install": work / "Программы Тест" / "Lumen VPN",
         "profile": work / "Пользователь Тест" / "профиль", "portable": False},
        {"name": "long-path", "install": work / LONG / "Lumen", "profile": work / LONG / "profile",
         "portable": False},
        {"name": "env-proxy", "install": work / "proxy" / "Lumen", "profile": work / "proxy" / "profile",
         "portable": False, "env": {"HTTP_PROXY": "http://127.0.0.1:9", "HTTPS_PROXY": "http://127.0.0.1:9",
                                    "ALL_PROXY": "socks5://127.0.0.1:9", "NO_PROXY": ""}},
        {"name": "light-persian", "install": work / "light" / "Lumen", "profile": work / "light" / "profile",
         "portable": False, "state": {"settings": {"theme": "light", "language": "fa",
                                                    "onboarding_revision": 99, "onboarding_version": "3.0.0"}}},
        {"name": "dark-chinese", "install": work / "zh" / "Lumen", "profile": work / "zh" / "profile",
         "portable": False, "state": {"settings": {"theme": "dark", "language": "zh",
                                                    "onboarding_revision": 99, "onboarding_version": "3.0.0"}}},
        {"name": "english-compact", "install": work / "en" / "Lumen", "profile": work / "en" / "profile",
         "portable": False, "state": {"settings": {"language": "en", "compact_mode": True,
                                                    "onboarding_revision": 99, "onboarding_version": "3.0.0"}}},
    ] if plain_profile else []


def data_dir(scenario: dict) -> Path:
    if scenario["portable"]:
        return scenario["install"] / "data"
    return scenario["profile"] / "Local" / "Lumen" / "data"


def run(scenario: dict, archive: Path, shots: Path) -> dict:
    install, profile = scenario["install"], scenario["profile"]
    for folder in (install, profile):
        if folder.exists():
            shutil.rmtree(folder)
    install.mkdir(parents=True)
    with zipfile.ZipFile(archive) as bundle:
        bundle.extractall(install)
    marker = install / "portable"
    if scenario["portable"]:
        marker.write_text("", encoding="utf-8")
    else:
        marker.unlink(missing_ok=True)
    (profile / "Local").mkdir(parents=True, exist_ok=True)
    (profile / "Roaming").mkdir(parents=True, exist_ok=True)
    env = {**os.environ, "LOCALAPPDATA": str(profile / "Local"), "APPDATA": str(profile / "Roaming"),
           **scenario.get("env", {})}
    if scenario.get("state"):
        # Seed the profile through the Python storage: it writes the same
        # state.enc the Rust client reads.
        seed = "; ".join([
            "import json, sys",
            "from xray_fluent.models import AppSettings",
            "from xray_fluent.storage import StateStorage",
            "storage = StateStorage()",
            "state = storage.load()",
            "data = state.settings.to_dict()",
            "data.update(json.loads(sys.argv[1]))",
            "state.settings = AppSettings.from_dict(data)",
            "storage.save(state)",
        ])
        subprocess.run([sys.executable, "-c", seed, json.dumps(scenario["state"]["settings"])],
                       cwd=HERE.parents[1] / "windows", env=env, check=True)
    exe = install / "Lumen.exe"
    result = {"name": scenario["name"], "install_path_chars": len(str(install))}
    log_path = shots / f"env-{scenario['name']}.log"
    with open(log_path, "w", encoding="utf-8", errors="replace") as log:
        process = subprocess.Popen([str(exe)], cwd=install, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            started = time.monotonic()
            hwnd = 0
            while time.monotonic() - started < 40 and not hwnd:
                handles = uicmp.windows_of(process.pid)
                hwnd = handles[0] if handles else 0
                if process.poll() is not None:
                    break
                time.sleep(0.1)
            result["window"] = bool(hwnd)
            result["startup_seconds"] = round(time.monotonic() - started, 2)
            if hwnd:
                # As uicmp does: a sized, positioned window repaints for PrintWindow.
                uicmp.user32.SetWindowPos(hwnd, 0, 60, 40, 1280, 760, 0x0004 | 0x0010)
                time.sleep(6)
                result["alive_after_6s"] = process.poll() is None
                if result["alive_after_6s"]:
                    uicmp.capture(hwnd, shots / f"env-{scenario['name']}.png", "print", 0)
            else:
                result["exit_code"] = process.poll()
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(8)
                except subprocess.TimeoutExpired:
                    process.kill()
    data = data_dir(scenario)
    result["data_dir_created"] = data.is_dir()
    result["data_files"] = sorted(path.name for path in data.iterdir())[:12] if data.is_dir() else []
    text = log_path.read_text(encoding="utf-8", errors="replace")
    logs = data / "logs"
    if logs.is_dir():
        for file in logs.glob("*"):
            if file.is_file():
                text += file.read_text(encoding="utf-8", errors="replace")
    result["panic_logged"] = "panicked at" in text
    return result


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    parser.add_argument("--zip", type=Path, default=HERE.parent / "dist" / "Lumen-portable-windows-x64.zip")
    parser.add_argument("--work", type=Path, default=HERE.parent / "target" / "env-matrix")
    parser.add_argument("--only")
    args = parser.parse_args()
    shots = args.work / "shots"
    shots.mkdir(parents=True, exist_ok=True)
    results = []
    for scenario in scenarios(args.work):
        if args.only and scenario["name"] != args.only:
            continue
        result = run(scenario, args.zip, shots)
        results.append(result)
        print(json.dumps(result, ensure_ascii=False))
    (shots / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0 if all(r.get("window") and r.get("alive_after_6s") and not r.get("panic_logged") for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
