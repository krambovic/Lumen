from __future__ import annotations

from collections import deque
import json
import os
import socket
import subprocess
import threading
import time
from pathlib import Path
from typing import Any

_CREATE_NO_WINDOW = 0x08000000 if os.name == "nt" else 0

from PyQt6.QtCore import QObject, pyqtSignal

from ...constants import (
    PROXY_HOST,
    RUNTIME_DIR,
    SINGBOX_CLASH_API_PORT,
    SINGBOX_CONFIG_FILE,
    SINGBOX_PATH_DEFAULT,
)
from ...path_utils import resolve_configured_path
from ...subprocess_utils import (
    decode_output,
    kill_processes_by_path,
    pump_qt_events,
    result_output_text,
    run_text_pumped,
    sleep_with_events,
)


class SingBoxManager(QObject):
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
        self._starting = False
        self._stop_requested = False
        self._startup_failure_reported = False
        self._runtime_error_reported = False
        self._last_output_lines: deque[str] = deque(maxlen=20)
        self._suppressed_noisy_lines = 0
        self._last_exit_code: int | None = None
        self._exe_path: Path | None = None

    @property
    def is_running(self) -> bool:
        return self._running

    def _proc_alive(self) -> bool:
        proc = self._proc
        return proc is not None and proc.poll() is None

    def start(self, singbox_path: str, config: dict[str, Any]) -> bool:
        exe = resolve_configured_path(
            singbox_path,
            default_path=SINGBOX_PATH_DEFAULT,
            use_default_if_empty=True,
            migrate_default_location=True,
        )
        if exe is None:
            self.error.emit("sing-box path is not configured (set it in Settings → Core paths)")
            return False
        if not exe.is_file():
            self.error.emit(f"sing-box.exe not found: {exe}")
            return False
        self._exe_path = exe

        tun_interface_name = self._extract_tun_interface_name(config)
        if not tun_interface_name:
            self.error.emit("sing-box config does not contain a TUN inbound interface_name")
            return False

        RUNTIME_DIR.mkdir(parents=True, exist_ok=True)
        SINGBOX_CONFIG_FILE.write_text(
            json.dumps(config, ensure_ascii=True, indent=2), encoding="utf-8"
        )

        if self._proc_alive():
            if not self.stop(expected=True):
                self.error.emit("failed to stop previous sing-box process")
                return False
        elif self._running:
            self._running = False
            self.state_changed.emit(False)

        # Kill only orphaned processes before start. The stable singbox_tun
        # adapter is reused by Wintun; retry cleanup handles real conflicts.
        self._kill_orphaned(exe)

        # A freshly killed sing-box can still hold the clash-api port; wait for it.
        self._wait_clash_api_port_released()

        # Set working directory to core/ so sing-box can find wintun.dll
        core_dir = exe.parent
        self._starting = True
        self._startup_failure_reported = False
        self._runtime_error_reported = False
        self._last_output_lines.clear()
        self._suppressed_noisy_lines = 0

        # Try up to 3 times — wintun adapter may need time to be released
        for attempt in range(3):
            attempt_started = time.monotonic()
            self.log_received.emit(f"[tun] startup attempt {attempt + 1}/3, interface={tun_interface_name}")
            self._last_output_lines.clear()
            self._stop_requested = False
            self._last_exit_code = None

            try:
                proc = subprocess.Popen(
                    [str(exe), "run", "-c", str(SINGBOX_CONFIG_FILE), "-D", str(core_dir)],
                    cwd=str(core_dir),
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    stdin=subprocess.DEVNULL,
                    bufsize=0,
                    creationflags=_CREATE_NO_WINDOW,
                )
            except Exception as exc:
                self._starting = False
                self._report_startup_failure(f"failed to start sing-box process: {exc}")
                return False

            with self._lock:
                self._proc = proc
            self._reader = threading.Thread(
                target=self._read_output,
                args=(proc,),
                name="singbox-output-reader",
                daemon=True,
            )
            self._reader.start()

            # sing-box is only considered "connected" once the TUN interface has
            # a usable IPv4 address — not merely when the process spawns.
            if self._wait_until_tun_ready(proc, tun_interface_name):
                adapter_ms = int((time.monotonic() - attempt_started) * 1000)
                self.log_received.emit(f"[tun] adapter and routes ready in {adapter_ms} ms")
                self._warm_windows_dns(proc)
                self._starting = False
                total_ms = int((time.monotonic() - attempt_started) * 1000)
                self.log_received.emit(
                    f"[tun] sing-box runtime ready in {total_ms} ms, interface={tun_interface_name}"
                )
                self._mark_running()
                return True

            exited = not self._proc_alive()
            retryable = exited and self._startup_error_is_retryable()
            if not exited:
                self.stop(expected=True)

            if retryable and attempt < 2:
                self._kill_orphaned(exe)
                self.cleanup_orphaned_tun_adapters()
                self._wait_tun_released()
                self._wait_clash_api_port_released()
                self._starting = True
                continue

            self._starting = False
            self.cleanup_orphaned_tun_adapters()
            if exited:
                self._report_startup_failure(
                    self._unexpected_exit_message(self._last_exit_code, startup=True)
                )
            else:
                self._report_startup_failure(
                    f"sing-box started but TUN interface '{tun_interface_name}' did not become ready in time"
                )
            return False

        self._starting = False
        return False

    @staticmethod
    def _kill_orphaned(exe: Path) -> None:
        """Kill orphaned sing-box processes that hold the TUN adapter."""
        if os.name != "nt":
            return
        try:
            if kill_processes_by_path(exe.name, exe, timeout=5):
                sleep_with_events(1.0)
        except Exception:
            pass

    def stop(self, expected: bool = True, *, fast: bool = False) -> bool:
        with self._lock:
            proc = self._proc
            if proc is None or proc.poll() is not None:
                self._stop_requested = False
                self._starting = False
                if self._running:
                    self._running = False
                    self.state_changed.emit(False)
                return True
            self._stop_requested = expected

        try:
            proc.terminate()
        except Exception:
            pass
        terminate_timeout = 0.8 if fast else 3.0
        kill_timeout = 0.5 if fast else 2.0
        orphan_timeout = 2 if fast else 5
        final_timeout = 0.3 if fast else 1.0
        release_timeout = 0.5 if fast else 1.0

        if not self._wait_proc(proc, terminate_timeout):
            try:
                proc.kill()
            except Exception:
                pass
            self._wait_proc(proc, kill_timeout)

        if proc.poll() is None:
            exe = self._exe_path
            if os.name == "nt" and exe is not None:
                try:
                    if kill_processes_by_path(exe.name, exe, timeout=orphan_timeout):
                        self._wait_proc(proc, final_timeout)
                except Exception:
                    pass

        if proc.poll() is None:
            self._stop_requested = False
            self.error.emit("failed to stop sing-box process in time")
            return False

        # Let sing-box/Wintun perform the normal adapter teardown. Disabling a
        # healthy adapter here makes Windows rebuild its network state on every
        # reconnect and delays the first real connections.
        self._starting = False
        self._wait_tun_released(max_wait=release_timeout)
        return True

    def _wait_proc(self, proc: subprocess.Popen[bytes], timeout_sec: float) -> bool:
        deadline = time.monotonic() + max(0.0, timeout_sec)
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                return True
            pump_qt_events()
            time.sleep(0.05)
        return proc.poll() is not None

    @staticmethod
    def _wait_tun_released(max_wait: float = 10.0) -> None:
        """Poll until the TUN adapter is gone, up to max_wait seconds."""
        if os.name != "nt":
            return
        step = 0.3
        waited = 0.0
        while waited < max_wait:
            try:
                result = run_text_pumped(
                    [
                        "powershell",
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        (
                            "$active = @(Get-NetAdapter -Name @('xftun*','singbox_tun') -ErrorAction SilentlyContinue "
                            "| Where-Object { $_.Status -notin @('Disabled','Not Present') }); "
                            "if ($active.Count -eq 0) { exit 0 } else { exit 1 }"
                        ),
                    ],
                    timeout=3,
                    check=False,
                    creationflags=_CREATE_NO_WINDOW,
                )
                if result.returncode == 0:
                    return
            except Exception:
                return  # can't check, proceed anyway
            sleep_with_events(step)
            waited += step

    @staticmethod
    def _wait_clash_api_port_released(max_wait: float = 5.0) -> None:
        """Wait until the clash-api controller port can be bound again."""
        if os.name != "nt":
            return
        waited = 0.0
        step = 0.2
        while waited < max_wait:
            probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            try:
                probe.bind((PROXY_HOST, SINGBOX_CLASH_API_PORT))
                return
            except OSError:
                pass
            finally:
                probe.close()
            sleep_with_events(step)
            waited += step

    @staticmethod
    def cleanup_orphaned_tun_adapters(max_wait: float = 5.0) -> None:
        """Remove routes from app-owned sing-box TUN adapters and disable them."""
        if os.name != "nt":
            return
        script = (
            "$ErrorActionPreference = 'SilentlyContinue'; "
            "$adapters = @(Get-NetAdapter -Name @('xftun*','singbox_tun') -ErrorAction SilentlyContinue); "
            "foreach ($adapter in $adapters) { "
            "$alias = $adapter.Name; "
            "Get-NetRoute -InterfaceAlias $alias -ErrorAction SilentlyContinue "
            "| Remove-NetRoute -Confirm:$false -ErrorAction SilentlyContinue; "
            "Disable-NetAdapter -Name $alias -Confirm:$false -ErrorAction SilentlyContinue; "
            "}"
        )
        try:
            run_text_pumped(
                ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
                timeout=max_wait,
                check=False,
                creationflags=_CREATE_NO_WINDOW,
            )
        except Exception:
            pass

    def _read_output(self, proc: subprocess.Popen[bytes]) -> None:
        stream = proc.stdout
        try:
            if stream is not None:
                for raw in iter(stream.readline, b""):
                    text = decode_output(raw)
                    for line in text.splitlines():
                        clean = line.rstrip()
                        if clean:
                            if not self._starting and self._is_noisy_runtime_line(clean):
                                self._suppressed_noisy_lines += 1
                                if self._suppressed_noisy_lines % 25 == 0:
                                    self.log_received.emit(
                                        f"[tun] {self._suppressed_noisy_lines} noisy connection logs suppressed..."
                                    )
                                continue
                            self._last_output_lines.append(clean)
                            self.log_received.emit(clean)
        except Exception:
            pass
        finally:
            try:
                proc.wait()
            except Exception:
                pass
            self._handle_process_exit(proc)

    def _handle_process_exit(self, proc: subprocess.Popen[bytes]) -> None:
        with self._lock:
            if proc is not self._proc:
                # A newer process superseded this handle; ignore the stale exit.
                return
            exit_code = proc.returncode if proc.returncode is not None else -1
            expected = self._stop_requested
            was_starting = self._starting
            was_running = self._running
            self._last_exit_code = exit_code
            self._stop_requested = False
            self._running = False
            self._proc = None

        if was_starting and not expected:
            self.cleanup_orphaned_tun_adapters()
            self._report_startup_failure(self._unexpected_exit_message(exit_code, startup=True))
        elif was_running and not expected and not self._runtime_error_reported:
            self.cleanup_orphaned_tun_adapters()
            self._runtime_error_reported = True
            self.error.emit(self._unexpected_exit_message(exit_code, startup=False))
        self.stopped.emit(exit_code)
        if was_running:
            self.state_changed.emit(False)

    def _mark_running(self) -> None:
        if self._running:
            return
        self._stop_requested = False
        self._running = True
        self.started.emit()
        self.state_changed.emit(True)

    @staticmethod
    def _extract_tun_interface_name(config: dict[str, Any]) -> str:
        for inbound in config.get("inbounds") or []:
            if not isinstance(inbound, dict):
                continue
            if str(inbound.get("type") or "").strip().lower() != "tun":
                continue
            return str(inbound.get("interface_name") or "").strip()
        return ""

    def _wait_until_tun_ready(
        self,
        proc: subprocess.Popen[bytes],
        tun_interface_name: str,
        max_wait: float = 8.0,
    ) -> bool:
        # Treat the runtime as ready only after Windows sees both the adapter
        # address and a broad route through it.
        deadline = time.monotonic() + max(0.2, max_wait)
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                return False
            if os.name != "nt" or self._tun_interface_has_ipv4(tun_interface_name):
                return True
            sleep_with_events(0.2)
        return False

    @staticmethod
    def _tun_interface_has_ipv4(tun_interface_name: str) -> bool:
        escaped_name = tun_interface_name.replace("'", "''")
        script = (
            f"$ipv4 = Get-NetIPAddress -InterfaceAlias '{escaped_name}' -AddressFamily IPv4 -ErrorAction SilentlyContinue "
            "| Where-Object { $_.IPAddress -and $_.IPAddress -ne '0.0.0.0' } "
            "| Select-Object -First 1 IPAddress; "
            f"$route = Get-NetRoute -InterfaceAlias '{escaped_name}' -AddressFamily IPv4 -ErrorAction SilentlyContinue "
            "| Where-Object { $_.DestinationPrefix -in @('0.0.0.0/0','0.0.0.0/1','128.0.0.0/1') } "
            "| Select-Object -First 1 DestinationPrefix; "
            "if ($ipv4 -and $route) { exit 0 } else { exit 1 }"
        )
        try:
            result = run_text_pumped(
                ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
                timeout=4,
                check=False,
                creationflags=_CREATE_NO_WINDOW,
            )
        except Exception:
            return False
        return result.returncode == 0

    def _warm_windows_dns(self, proc: subprocess.Popen[bytes]) -> None:
        if os.name != "nt" or proc.poll() is not None:
            return
        started = time.monotonic()
        script = (
            "$ErrorActionPreference = 'Stop'; "
            "Clear-DnsClientCache; "
            "$answer = Resolve-DnsName -Name 'www.gstatic.com' -Type A -DnsOnly -QuickTimeout "
            "| Where-Object { $_.IPAddress } | Select-Object -First 1; "
            "if ($answer) { Write-Output $answer.IPAddress; exit 0 } else { exit 1 }"
        )
        self.log_received.emit("[tun] refreshing Windows DNS cache and warming resolver...")
        try:
            result = run_text_pumped(
                ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
                timeout=4,
                check=False,
                creationflags=_CREATE_NO_WINDOW,
            )
            elapsed_ms = int((time.monotonic() - started) * 1000)
            output = result_output_text(result).strip().splitlines()
            if result.returncode == 0:
                answer = output[-1].strip() if output else "ok"
                self.log_received.emit(f"[tun] DNS warm-up ready in {elapsed_ms} ms ({answer})")
            else:
                self.log_received.emit(f"[tun] DNS warm-up did not answer in {elapsed_ms} ms; continuing")
        except Exception as exc:
            elapsed_ms = int((time.monotonic() - started) * 1000)
            self.log_received.emit(f"[tun] DNS warm-up skipped after {elapsed_ms} ms: {exc}")

    def _startup_error_is_retryable(self) -> bool:
        needles = (
            "already exists",
            "cannot create a file when that file already exists",
            "only one usage of each socket address",
            "external controller listen error",
            "address already in use",
            "bind:",
        )
        for line in self._last_output_lines:
            text = line.lower()
            if any(needle in text for needle in needles):
                return True
        return False

    def _unexpected_exit_message(
        self,
        exit_code: int | None,
        *,
        startup: bool,
    ) -> str:
        stage = "during startup" if startup else "unexpectedly"
        detail = self._last_output_lines[-1].strip() if self._last_output_lines else ""
        if detail:
            return f"sing-box exited {stage}: {detail}"
        if exit_code is None:
            return f"sing-box exited {stage}."
        status_name = "CrashExit" if exit_code < 0 else "NormalExit"
        return f"sing-box exited {stage} with code {exit_code} ({status_name})."

    def _report_startup_failure(self, message: str) -> None:
        if self._startup_failure_reported:
            return
        self._startup_failure_reported = True
        self.error.emit(message)

    @staticmethod
    def _is_noisy_runtime_line(line: str) -> bool:
        text = line.lower()
        if any(marker in text for marker in ("error", "failed", "timeout", "deadline", "fatal", "panic")):
            return False
        if "connection upload closed" in text or "connection download closed" in text:
            return True
        if "an existing connection was forcibly closed by the remote host" in text:
            return True
        if "wsarecv" in text or "wsasend" in text:
            return True
        return False


def get_singbox_version(singbox_path: str) -> str | None:
    exe = resolve_configured_path(
        singbox_path,
        default_path=SINGBOX_PATH_DEFAULT,
        use_default_if_empty=True,
        migrate_default_location=True,
    )
    if exe is None:
        return None
    if not exe.exists():
        return None
    try:
        result = run_text_pumped(
            [str(exe), "version"],
            timeout=3,
            check=False,
            creationflags=_CREATE_NO_WINDOW,
        )
    except Exception:
        return None

    lines = result_output_text(result).splitlines()
    if not lines:
        return None
    return lines[0].strip()
