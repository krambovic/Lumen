"""Side-by-side visual comparison of the Rust (egui) and QML clients.

Both clients run against an isolated profile: LOCALAPPDATA and APPDATA point
into rust/target/uicmp/profiles/<name>, so the user's real profile is never
touched. The QML client starts with --side-by-side (no single-instance mutex).

    python rust/tools/uicmp/uicmp.py stage
    python rust/tools/uicmp/uicmp.py run rust --profile fresh --fresh \
        --size 1280x760 --shots 3,4.5 --out onboarding
    python rust/tools/uicmp/uicmp.py run qml --profile fresh --shots 6

Input:
- Rust (debug build): --input "1.5,click,640,400;2,key,Escape" is handed over
  through LUMEN_PREVIEW_INPUT_FILE when the window appears (see
  crates/lumen-ui/src/shell/preview_input.rs); times count from then.
  Coordinates are egui points inside the window.
- QML: --post "1.5,click,640,400;2,rclick,10,10;3,wheel,640,400,-120" posts
  window messages to the Qt window (client pixels). Neither moves the real
  cursor or touches other windows.

Capture: --mode print (default) uses PrintWindow, which works while the
window is covered; the DWM backdrop shows as black. --mode screen grabs the
screen region instead (the only way to see the backdrop, shadow and rounded
corners); it brings the window to the front first, so use it only on an
idle machine. --mode top makes the window topmost without activating it for
the grab and sends it to the bottom afterwards.
"""

from __future__ import annotations

import argparse
import ctypes
import os
import shutil
import subprocess
import sys
import time
from ctypes import wintypes
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
RUST = ROOT / "rust"
WINDOWS = ROOT / "windows"
WORK = RUST / "target" / "uicmp"
DEBUG_DIR = RUST / "target" / "debug"

user32 = ctypes.WinDLL("user32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32")
try:
    ctypes.WinDLL("shcore").SetProcessDpiAwareness(2)
except Exception:
    pass

WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)


def _junction(link: Path, target: Path) -> None:
    if link.exists():
        return
    link.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cmd", "/c", "mklink", "/J", str(link), str(target)], check=True,
                   stdout=subprocess.DEVNULL)


def stage() -> None:
    """Make target/debug look like an installed app (cores, assets, templates)."""
    _junction(DEBUG_DIR / "core", WINDOWS / "core")
    _junction(DEBUG_DIR / "zapret", WINDOWS / "zapret")
    _junction(DEBUG_DIR / "assets", WINDOWS / "assets")
    _junction(DEBUG_DIR / "data" / "templates", WINDOWS / "data" / "templates")


def windows_of(pid: int) -> list[int]:
    found: list[int] = []

    def callback(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            length = user32.GetWindowTextLengthW(hwnd)
            title = ctypes.create_unicode_buffer(length + 1)
            user32.GetWindowTextW(hwnd, title, length + 1)
            if title.value == "Lumen":
                found.append(hwnd)
        return True

    user32.EnumWindows(WNDENUMPROC(callback), 0)
    return found


def window_rect(hwnd: int) -> tuple[int, int, int, int]:
    rect = wintypes.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    return rect.left, rect.top, rect.right, rect.bottom


def capture(hwnd: int, path: Path, mode: str, margin: int) -> None:
    from PIL import Image, ImageGrab

    left, top, right, bottom = window_rect(hwnd)
    if mode in ("screen", "top"):
        if mode == "top":
            # Topmost without activation for the grab only, then back.
            user32.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010)
            time.sleep(0.25)
        else:
            user32.SetForegroundWindow(hwnd)
            time.sleep(0.05)
        image = ImageGrab.grab(bbox=(left - margin, top - margin, right + margin, bottom + margin),
                               all_screens=True)
        if mode == "top":
            user32.SetWindowPos(hwnd, -2, 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010)
            user32.SetWindowPos(hwnd, 1, 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010)
    else:
        width, height = right - left, bottom - top
        hdc = user32.GetWindowDC(hwnd)
        memdc = gdi32.CreateCompatibleDC(hdc)
        bitmap = gdi32.CreateCompatibleBitmap(hdc, width, height)
        gdi32.SelectObject(memdc, bitmap)
        user32.PrintWindow(hwnd, memdc, 2)
        header = (ctypes.c_uint32 * 10)(40, width, (-height) & 0xFFFFFFFF, 1 | (32 << 16), 0, 0, 0, 0, 0, 0)
        buffer = ctypes.create_string_buffer(width * height * 4)
        gdi32.GetDIBits(memdc, bitmap, 0, height, buffer, header, 0)
        image = Image.frombuffer("RGBA", (width, height), buffer, "raw", "BGRA", 0, 1).convert("RGB")
        gdi32.DeleteObject(bitmap)
        gdi32.DeleteDC(memdc)
        user32.ReleaseDC(hwnd, hdc)
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path)
    print(path)


def post_steps(script: str) -> list[tuple[float, list[str]]]:
    steps = []
    for item in filter(None, (part.strip() for part in script.split(";"))):
        parts = [value.strip() for value in item.split(",")]
        steps.append((float(parts[0]), parts[1:]))
    return sorted(steps, key=lambda step: step[0])


def post(hwnd: int, args: list[str]) -> None:
    WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP = 0x200, 0x201, 0x202
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_MOUSEWHEEL = 0x204, 0x205, 0x20A
    WM_KEYDOWN, WM_KEYUP = 0x100, 0x101
    action = args[0]
    if action in ("click", "rclick", "move", "wheel"):
        x, y = int(args[1]), int(args[2])
        lparam = (y << 16) | (x & 0xFFFF)
        user32.PostMessageW(hwnd, WM_MOUSEMOVE, 0, lparam)
        if action == "click":
            user32.PostMessageW(hwnd, WM_LBUTTONDOWN, 1, lparam)
            user32.PostMessageW(hwnd, WM_LBUTTONUP, 0, lparam)
        elif action == "rclick":
            user32.PostMessageW(hwnd, WM_RBUTTONDOWN, 2, lparam)
            user32.PostMessageW(hwnd, WM_RBUTTONUP, 0, lparam)
        elif action == "wheel":
            left, top, _, _ = window_rect(hwnd)
            delta = int(args[3]) & 0xFFFF
            screen = ((top + y) << 16) | ((left + x) & 0xFFFF)
            user32.PostMessageW(hwnd, WM_MOUSEWHEEL, delta << 16, screen)
    elif action == "key":
        vk = {"Escape": 0x1B, "Enter": 0x0D, "Tab": 0x09, "Down": 0x28, "Up": 0x26}[args[1]]
        user32.PostMessageW(hwnd, WM_KEYDOWN, vk, 1)
        user32.PostMessageW(hwnd, WM_KEYUP, vk, 0xC0000001)


def run(args) -> int:
    profile = WORK / "profiles" / args.profile
    if (args.fresh or args.clone) and profile.exists():
        shutil.rmtree(profile)
    if args.clone:
        # The QML client copies windows/data into an empty profile (state path
        # migration); cloning a Rust-made profile gives both the same state.
        shutil.copytree(WORK / "profiles" / args.clone, profile)
    (profile / "Local").mkdir(parents=True, exist_ok=True)
    (profile / "Roaming").mkdir(parents=True, exist_ok=True)
    env = {**os.environ, "LOCALAPPDATA": str(profile / "Local"), "APPDATA": str(profile / "Roaming")}
    if args.client == "rust":
        stage()
        exe = DEBUG_DIR / "Lumen.exe"
        script_file = WORK / "preview-input.txt"
        script_file.unlink(missing_ok=True)
        if args.input:
            # Written once the window is shown, so input and captures share
            # one clock (see preview_input.rs).
            env["LUMEN_PREVIEW_INPUT_FILE"] = str(script_file)
        command = [str(exe), *args.extra]
        cwd = DEBUG_DIR
    else:
        command = [sys.executable, str(WINDOWS / "run_qml.py"), "--side-by-side", *args.extra]
        cwd = WINDOWS
    log = open(WORK / f"{args.client}.log", "w", encoding="utf-8", errors="replace")
    process = subprocess.Popen(command, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT)
    started = time.monotonic()
    hwnd = 0
    try:
        while time.monotonic() - started < 40:
            handles = windows_of(process.pid)
            if handles:
                hwnd = handles[0]
                break
            if process.poll() is not None:
                print("client exited", process.returncode)
                return 1
            time.sleep(0.05)
        if not hwnd:
            print("no window")
            return 1
        shown = time.monotonic()
        if args.client == "rust" and args.input:
            (WORK / "preview-input.txt").write_text(args.input, encoding="utf-8")
        print(f"hwnd {hwnd:#x} style {user32.GetWindowLongW(hwnd, -16) & 0xFFFFFFFF:#010x} "
              f"exstyle {user32.GetWindowLongW(hwnd, -20) & 0xFFFFFFFF:#010x}")
        if args.size:
            width, height = (int(value) for value in args.size.split("x"))
            user32.SetWindowPos(hwnd, 0, 60, 40, width, height, 0x0004 | 0x0010)
        events: list[tuple[float, str, object]] = []
        for at in filter(None, args.shots.split(",")):
            events.append((float(at), "shot", None))
        if args.post:
            for at, step in post_steps(args.post):
                events.append((at, "post", step))
        events.sort(key=lambda event: event[0])
        for index, (at, kind, payload) in enumerate(events):
            delay = shown + at - time.monotonic()
            if delay > 0:
                time.sleep(delay)
            if kind == "post":
                post(hwnd, payload)
            else:
                name = f"{args.out}-{args.client}-{at:g}.png"
                capture(hwnd, WORK / "shots" / name, args.mode, args.margin)
        if args.keep:
            return 0
    finally:
        if not args.keep:
            process.terminate()
            try:
                process.wait(5)
            except subprocess.TimeoutExpired:
                process.kill()
        log.close()
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("stage")
    runner = sub.add_parser("run")
    runner.add_argument("client", choices=("rust", "qml"))
    runner.add_argument("--profile", default="default")
    runner.add_argument("--fresh", action="store_true", help="delete the profile first")
    runner.add_argument("--clone", default="", help="start from a copy of another profile")
    runner.add_argument("--size", default="1280x760")
    runner.add_argument("--shots", default="4", help="seconds after the window appears")
    runner.add_argument("--out", default="shot")
    runner.add_argument("--mode", choices=("screen", "top", "print"), default="print")
    runner.add_argument("--margin", type=int, default=0)
    runner.add_argument("--input", default="", help="Rust scripted input")
    runner.add_argument("--post", default="", help="QML posted window messages")
    runner.add_argument("--keep", action="store_true", help="leave the client running")
    runner.add_argument("extra", nargs="*")
    args = parser.parse_args()
    WORK.mkdir(parents=True, exist_ok=True)
    if args.command == "stage":
        stage()
        return 0
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
