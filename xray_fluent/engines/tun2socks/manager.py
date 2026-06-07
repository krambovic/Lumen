from __future__ import annotations

import os
import subprocess
import threading
import time
from urllib.parse import quote

_CREATE_NO_WINDOW = 0x08000000 if os.name == "nt" else 0

from PyQt6.QtCore import QObject, pyqtSignal

from ...constants import BASE_DIR
from ...subprocess_utils import (
    decode_output,
    kill_processes_by_path,
    pump_qt_events,
    result_output_text,
    run_text_pumped,
    sleep_with_events,
)

TUN2SOCKS_PATH_DEFAULT = BASE_DIR / "core" / "tun2socks.exe"
TUN_DEVICE_NAME = "BebraVPN_TUN"
TUN_GW = "172.19.0.1"
TUN_ADDR = "172.19.0.2"
TUN_MASK = "255.255.255.252"
TUN_GW6 = "fd00::1"
TUN_CIDR = "172.19.0.1/30"


class Tun2SocksManager(QObject):
    started = pyqtSignal()
    stopped = pyqtSignal(int)
    log_received = pyqtSignal(str)
    error = pyqtSignal(str)
    state_changed = pyqtSignal(bool)

    def __init__(self, parent: QObject | None = None):
        super().__init__(parent)
        self._proc: subprocess.Popen[bytes] | None = None
        self._reader: threading.Thread | None = None
        self._lock = threading.RLock()
        self._running = False
        self._stop_requested = False
        self._server_ip: str = ""
        self._orig_gateway: str = ""
        self._tun_idx: str = ""
        self._helper_routes: list[list[str]] = []

    @property
    def is_running(self) -> bool:
        return self._running

    def _proc_alive(self) -> bool:
        proc = self._proc
        return proc is not None and proc.poll() is None

    def start(self, socks_port: int, *, username: str = "", password: str = "", server_ip: str = "") -> bool:
        exe = TUN2SOCKS_PATH_DEFAULT
        if not exe.is_file():
            self.error.emit(f"tun2socks.exe not found: {exe}")
            return False

        self._server_ip = server_ip

        if self._proc_alive():
            if not self.stop(expected=True):
                self.error.emit("failed to stop previous tun2socks process")
                return False
        elif self._running:
            self._running = False
            self.state_changed.emit(False)

        # Kill orphaned tun2socks
        self._kill_orphaned()

        proxy_url = f"socks5://127.0.0.1:{socks_port}"
        if username or password:
            proxy_url = f"socks5://{quote(username, safe='')}:{quote(password, safe='')}@127.0.0.1:{socks_port}"

        args = [
            str(exe),
            "-device", f"tun://{TUN_DEVICE_NAME}",
            "-proxy", proxy_url,
            "-loglevel", "error",
        ]

        try:
            proc = subprocess.Popen(
                args,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                stdin=subprocess.DEVNULL,
                bufsize=0,
                creationflags=_CREATE_NO_WINDOW,
            )
        except Exception as exc:
            self.error.emit(f"failed to start tun2socks: {exc}")
            return False

        with self._lock:
            self._proc = proc
            self._stop_requested = False
            reader = threading.Thread(
                target=self._read_output,
                args=(proc,),
                name="tun2socks-output-reader",
                daemon=True,
            )
            self._reader = reader
            reader.start()

        self._on_started(proc)

        # Give it a moment to boot / fail fast
        sleep_with_events(0.5)
        if not self._proc_alive():
            self.error.emit("tun2socks exited right after start")
            return False

        # Wait until TUN interface appears (up to 10 seconds)
        for _ in range(20):
            if not self._proc_alive():
                self.error.emit("tun2socks exited right after start")
                return False
            result = run_text_pumped(
                ["netsh", "interface", "ipv4", "show", "interfaces"],
                timeout=5,
                creationflags=_CREATE_NO_WINDOW,
            )
            if TUN_DEVICE_NAME in result_output_text(result):
                break
            sleep_with_events(0.5)
        else:
            self.error.emit("TUN adapter did not appear after tun2socks start")
            self.stop(expected=True)
            return False

        # Configure routes
        if not self._setup_routes():
            self.stop(expected=True)
            return False
        return True

    def stop(self, expected: bool = True, *, fast: bool = False) -> bool:
        proc = self._proc
        if not self._proc_alive():
            self._stop_requested = False
            if self._running:
                self._running = False
                self.state_changed.emit(False)
            self._cleanup_routes()
            return True

        self._stop_requested = expected
        try:
            proc.terminate()
        except Exception:
            pass
        terminate_timeout = 0.6 if fast else 2.0
        kill_timeout = 0.4 if fast else 1.0

        if self._wait_proc(proc, terminate_timeout):
            self._finish_stop(fast=fast)
            return True

        try:
            proc.kill()
        except Exception:
            pass
        if self._wait_proc(proc, kill_timeout):
            self._finish_stop(fast=fast)
            return True

        if not self._proc_alive():
            self._finish_stop(fast=fast)
            return True

        self._stop_requested = False
        self.error.emit("failed to stop tun2socks in time")
        return False

    def _finish_stop(self, *, fast: bool = False) -> None:
        reader = self._reader
        if reader is not None and reader.is_alive() and reader is not threading.current_thread():
            reader.join(timeout=0.5 if fast else 2.0)
        self._cleanup_routes(timeout=1 if fast else 5)

    def _wait_proc(self, proc: subprocess.Popen[bytes], timeout: float) -> bool:
        deadline = time.monotonic() + max(0.0, timeout)
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                return True
            pump_qt_events()
            time.sleep(0.05)
        return proc.poll() is not None

    def _read_output(self, proc: subprocess.Popen[bytes]) -> None:
        stream = proc.stdout
        try:
            if stream is not None:
                for raw in iter(stream.readline, b""):
                    text = decode_output(raw)
                    for line in text.splitlines():
                        clean = line.rstrip()
                        if clean:
                            self.log_received.emit(clean)
        except Exception:
            pass
        finally:
            try:
                proc.wait()
            except Exception:
                pass
            self._handle_process_exit(proc)

    def _on_started(self, proc: subprocess.Popen[bytes]) -> None:
        with self._lock:
            if proc is not self._proc or proc.poll() is not None:
                return
            if self._running:
                return
            self._stop_requested = False
            self._running = True
        self.started.emit()
        self.state_changed.emit(True)

    def _handle_process_exit(self, proc: subprocess.Popen[bytes]) -> None:
        with self._lock:
            if proc is not self._proc:
                return
            exit_code = proc.poll()
            if exit_code is None:
                exit_code = -1
            self._proc = None
            self._reader = None
            self._stop_requested = False
            was_running = self._running
            self._running = False
        self._cleanup_routes()
        self.stopped.emit(int(exit_code))
        if was_running:
            self.state_changed.emit(False)

    def _setup_routes(self) -> bool:
        """Set up routes so all traffic goes through the TUN adapter."""
        if os.name != "nt":
            return True
        try:
            self._helper_routes = []
            # Find TUN interface index by name
            result = run_text_pumped(
                ["netsh", "interface", "ipv4", "show", "interfaces"],
                timeout=5,
                creationflags=_CREATE_NO_WINDOW,
            )
            tun_idx = ""
            for line in result_output_text(result).splitlines():
                if TUN_DEVICE_NAME in line:
                    parts = line.split()
                    if parts and parts[0].isdigit():
                        tun_idx = parts[0]
                        break
            if not tun_idx:
                self.error.emit("failed to detect TUN interface index")
                return False

            # Get current default gateway
            result = run_text_pumped(
                ["cmd", "/c", "route", "print", "0.0.0.0"],
                timeout=5,
                creationflags=_CREATE_NO_WINDOW,
            )
            orig_gw = ""
            for line in result_output_text(result).splitlines():
                parts = line.split()
                if len(parts) >= 5 and parts[0] == "0.0.0.0" and parts[1] == "0.0.0.0":
                    orig_gw = parts[2]
                    break
            if not orig_gw:
                self.error.emit("failed to detect current default gateway")
                return False
            self._orig_gateway = orig_gw
            self._tun_idx = tun_idx

            # Set TUN interface metric very low so it wins
            run_text_pumped(
                ["netsh", "interface", "ipv4", "set", "interface", tun_idx, "metric=1"],
                timeout=5,
                creationflags=_CREATE_NO_WINDOW,
            )

            cmds: list[list[str]] = []
            if self._server_ip:
                cmds.append(["route", "add", self._server_ip, "mask", "255.255.255.255", orig_gw, "metric", "1"])
            helper_routes = [
                [orig_gw, "255.255.255.255"],
                ["192.168.0.0", "255.255.0.0"],
                ["10.0.0.0", "255.0.0.0"],
                ["172.16.0.0", "255.240.0.0"],
                ["169.254.0.0", "255.255.0.0"],
            ]
            for destination, mask in helper_routes:
                cmd = ["route", "add", destination, "mask", mask, orig_gw, "metric", "1"]
                cmds.append(cmd)
                self._helper_routes.append([destination, mask, orig_gw])

            cleanup_cmds = [
                ["route", "delete", "0.0.0.0", "mask", "128.0.0.0", TUN_GW],
                ["route", "delete", "128.0.0.0", "mask", "128.0.0.0", TUN_GW],
                ["netsh", "interface", "ipv4", "delete", "route", "0.0.0.0/1", f"interface={tun_idx}"],
                ["netsh", "interface", "ipv4", "delete", "route", "128.0.0.0/1", f"interface={tun_idx}"],
                ["netsh", "interface", "ipv6", "delete", "route", "::/0", f"interface={tun_idx}"],
            ]
            if self._server_ip:
                cleanup_cmds.append(["route", "delete", self._server_ip])
            for destination, mask, gateway in self._helper_routes:
                cleanup_cmds.append(["route", "delete", destination, "mask", mask, gateway])
            for cmd in cleanup_cmds:
                run_text_pumped(cmd, timeout=5, creationflags=_CREATE_NO_WINDOW)

            # Use netsh to add TUN routes — this correctly sets interface metric
            cmds += [
                ["netsh", "interface", "ipv4", "add", "route", "0.0.0.0/1", f"interface={tun_idx}", f"nexthop={TUN_GW}", "metric=0"],
                ["netsh", "interface", "ipv4", "add", "route", "128.0.0.0/1", f"interface={tun_idx}", f"nexthop={TUN_GW}", "metric=0"],
                ["netsh", "interface", "ipv6", "add", "route", "::/0", f"interface={tun_idx}", "metric=1"],
            ]
            for cmd in cmds:
                r = run_text_pumped(cmd, timeout=5, creationflags=_CREATE_NO_WINDOW)
                self.log_received.emit(f"[tun2socks] {' '.join(cmd)} -> rc={r.returncode}")
                if r.returncode != 0:
                    details = result_output_text(r).strip()
                    if details:
                        self.log_received.emit(f"[tun2socks] command output: {details}")
                    self._cleanup_routes()
                    self.error.emit(f"failed to configure route: {' '.join(cmd)}")
                    return False
            return True
        except Exception as exc:
            self._cleanup_routes()
            self.log_received.emit(f"[tun2socks] route setup error: {exc}")
            return False

    def _cleanup_routes(self, *, timeout: int = 5) -> None:
        """Remove routes added by _setup_routes."""
        if os.name != "nt":
            return
        try:
            cmds = [
                ["route", "delete", "0.0.0.0", "mask", "128.0.0.0", TUN_GW],
                ["route", "delete", "128.0.0.0", "mask", "128.0.0.0", TUN_GW],
            ]
            if hasattr(self, '_tun_idx') and self._tun_idx:
                cmds += [
                    ["netsh", "interface", "ipv4", "delete", "route", "0.0.0.0/1", f"interface={self._tun_idx}"],
                    ["netsh", "interface", "ipv4", "delete", "route", "128.0.0.0/1", f"interface={self._tun_idx}"],
                    ["netsh", "interface", "ipv6", "delete", "route", "::/0", f"interface={self._tun_idx}"],
                ]
            if self._server_ip:
                cmds.append(["route", "delete", self._server_ip])
            for destination, mask, gateway in self._helper_routes:
                cmds.append(["route", "delete", destination, "mask", mask, gateway])
            for cmd in cmds:
                run_text_pumped(cmd, timeout=timeout, creationflags=_CREATE_NO_WINDOW)
            self._helper_routes = []
        except Exception:
            pass

    @staticmethod
    def _kill_orphaned() -> None:
        if os.name != "nt":
            return
        try:
            if kill_processes_by_path("tun2socks.exe", TUN2SOCKS_PATH_DEFAULT, timeout=5):
                sleep_with_events(1.0)
        except Exception:
            pass
