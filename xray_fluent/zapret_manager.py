"""Minimal winws2 (zapret2) process manager — preset-based, no orchestrator."""

from __future__ import annotations

import logging
import os
import hashlib
import re
import shlex
import shutil
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

if os.name == "nt":
    import winreg

from PyQt6.QtCore import QObject, QProcess, QTimer, pyqtSignal

from .constants import BASE_DIR
from .subprocess_utils import (
    CREATE_NO_WINDOW,
    decode_output,
    kill_processes_by_path,
    run_text_pumped,
    sleep_with_events,
    wait_for_qprocess_started,
    wait_for_qprocess_finished,
)

log = logging.getLogger(__name__)

ZAPRET_DIR = BASE_DIR / "zapret"
WINWS2_EXE = ZAPRET_DIR / "exe" / "winws2.exe"
WINWS_EXE = ZAPRET_DIR / "exe" / "winws.exe"
PRESETS_DIR = ZAPRET_DIR / "presets"
PROGRAM_DATA_DIR = Path(os.environ.get("ProgramData") or r"C:\ProgramData")
AT_CONFIG_DIR = PROGRAM_DATA_DIR / "LumenKVN" / "zapret" / "winws2_at_config"
_INLINE_ARG_SPLIT_RE = re.compile(r"(?<=\S)\s+(?=--)")
_LIST_FILE_ARG_RE = re.compile(r"^--(?:ipset|ipset-exclude|hostlist|hostlist-exclude)=(.+)$")
_ELEVATION_ERROR_MARKERS = (
    "elevation",
    "requires elevation",
    "operation requires elevation",
    "requested operation requires elevation",
    "740",
    "требует повышения",
    "требуются повышенные права",
)


def _is_elevation_launch_error(text: str) -> bool:
    lowered = (text or "").lower()
    return any(marker in lowered for marker in _ELEVATION_ERROR_MARKERS)


_WINDIVERT_CONFLICT_MARKERS = (
    "windivert",
    "another instance",
    "already running",
    "error opening filter",
    "failed to open filter",
    "не удалось открыть",
    "другой экземпляр",
)

@dataclass
class PresetInfo:
    name: str
    description: str
    created: str
    modified: str
    arg_count: int
    file_path: Path


@dataclass(frozen=True)
class _PresetArgsCacheEntry:
    mtime_ns: int
    size: int
    args: tuple[str, ...]


class ZapretManager(QObject):
    """Start / stop winws2.exe with a preset file."""

    started = pyqtSignal()
    stopped = pyqtSignal()
    error = pyqtSignal(str)
    log_line = pyqtSignal(str)

    _preset_names_cache: tuple[tuple[tuple[str, int, int], ...], list[str]] | None = None
    _preset_infos_cache: tuple[tuple[tuple[str, int, int], ...], list[PresetInfo]] | None = None
    _preset_args_cache: dict[str, _PresetArgsCacheEntry] = {}
    _windivert_services_cache: list[str] | None = None
    _standard_cleanup_done: bool = False

    def __init__(self, parent: QObject | None = None):
        super().__init__(parent)
        self._process: QProcess | None = None
        self._current_preset: str = ""
        self._start_args: list[str] = []
        self._start_retry_count = 0
        self._current_at_config: Path | None = None
        self._launch_mode = "direct"
        self._output_tail: list[str] = []
        self._health_timer = QTimer(self)
        self._health_timer.setInterval(3000)
        self._health_timer.timeout.connect(self._check_health)

    @staticmethod
    def _presets_signature() -> tuple[tuple[str, int, int], ...]:
        if not PRESETS_DIR.is_dir():
            return ()
        entries: list[tuple[str, int, int]] = []
        for path in PRESETS_DIR.iterdir():
            if path.suffix != ".txt" or path.name.startswith("_"):
                continue
            try:
                stat = path.stat()
                entries.append((path.name, int(stat.st_mtime_ns), int(stat.st_size)))
            except OSError:
                continue
        return tuple(sorted(entries))

    @classmethod
    def invalidate_preset_cache(cls) -> None:
        cls._preset_names_cache = None
        cls._preset_infos_cache = None
        cls._preset_args_cache.clear()

    # ── public API ──────────────────────────────────────────────

    @property
    def running(self) -> bool:
        return self._process is not None and self._process.state() == QProcess.ProcessState.Running

    @staticmethod
    def list_presets() -> list[str]:
        """Return sorted list of available preset names (without .txt)."""
        signature = ZapretManager._presets_signature()
        cached = ZapretManager._preset_names_cache
        if cached and cached[0] == signature:
            return list(cached[1])
        names = sorted(path_name[:-4] for path_name, _mtime, _size in signature)
        ZapretManager._preset_names_cache = (signature, names)
        return list(names)

    @staticmethod
    def preset_path(name: str) -> Path:
        return PRESETS_DIR / f"{name}.txt"

    @staticmethod
    def _split_launch_line(raw_line: str) -> list[str]:
        stripped = str(raw_line or "").strip()
        if not stripped:
            return []
        if not stripped.startswith("--"):
            return [stripped]
        return [part.strip() for part in _INLINE_ARG_SPLIT_RE.split(stripped) if part.strip()]

    @staticmethod
    def _parse_preset_args(preset: Path) -> list[str]:
        """Read preset file and return argv items like upstream winws2 runner."""
        try:
            stat = preset.stat()
            cache_key = str(preset.resolve())
            cached = ZapretManager._preset_args_cache.get(cache_key)
            if cached and cached.mtime_ns == stat.st_mtime_ns and cached.size == stat.st_size:
                return list(cached.args)
        except OSError:
            stat = None
            cache_key = str(preset)
        args: list[str] = []
        text = preset.read_text(encoding="utf-8", errors="replace")
        for line in text.splitlines():
            stripped = line.strip()
            if stripped and not stripped.startswith("#"):
                args.extend(ZapretManager._split_launch_line(stripped))
        if stat is not None:
            ZapretManager._preset_args_cache[cache_key] = _PresetArgsCacheEntry(
                int(stat.st_mtime_ns),
                int(stat.st_size),
                tuple(args),
            )
        return args

    @staticmethod
    def _write_at_config(preset: Path, args: list[str]) -> Path:
        config_text = "\n".join(shlex.quote(arg) for arg in args if str(arg or "").strip()) + "\n"
        digest_source = f"{preset.resolve()}\0{config_text}".encode("utf-8", "surrogatepass")
        digest = hashlib.sha1(digest_source).hexdigest()[:20]
        AT_CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        config_path = AT_CONFIG_DIR / f"winws2_at_{digest}.txt"
        try:
            if config_path.read_text(encoding="utf-8", errors="replace") == config_text:
                ZapretManager._prune_at_config_cache(config_path)
                return config_path
        except FileNotFoundError:
            pass
        except Exception:
            pass
        config_path.write_text(config_text, encoding="utf-8", newline="\n")
        ZapretManager._prune_at_config_cache(config_path)
        return config_path

    @staticmethod
    def _prune_at_config_cache(keep_path: Path, *, max_files: int = 64) -> None:
        try:
            entries = [
                p for p in AT_CONFIG_DIR.iterdir()
                if p.is_file() and p.name.startswith("winws2_at_") and p.suffix == ".txt"
            ]
            entries.sort(key=lambda p: p.stat().st_mtime, reverse=True)
            keep = keep_path.resolve()
            for path in entries[max(1, max_files):]:
                if path.resolve() != keep:
                    path.unlink(missing_ok=True)
        except Exception:
            pass

    @staticmethod
    def _parse_metadata(text: str) -> dict[str, str]:
        """Extract metadata from comment headers."""
        meta: dict[str, str] = {}
        for line in text.splitlines()[:15]:  # only check first 15 lines
            stripped = line.strip()
            if not stripped.startswith("#"):
                if stripped:  # non-empty non-comment = end of headers
                    break
                continue
            for key in ("Preset", "Description", "Created", "Modified", "BuiltinVersion"):
                prefix = f"# {key}:"
                if stripped.startswith(prefix):
                    meta[key] = stripped[len(prefix):].strip()
                    break
        return meta

    @staticmethod
    def list_preset_infos() -> list[PresetInfo]:
        """Return list of PresetInfo for all presets, sorted by name."""
        signature = ZapretManager._presets_signature()
        cached = ZapretManager._preset_infos_cache
        if cached and cached[0] == signature:
            return list(cached[1])
        if not PRESETS_DIR.is_dir():
            ZapretManager._preset_infos_cache = (signature, [])
            return []
        result = []
        for p in sorted(PRESETS_DIR.iterdir()):
            if p.suffix != ".txt" or p.name.startswith("_"):
                continue
            text = p.read_text(encoding="utf-8", errors="replace")
            meta = ZapretManager._parse_metadata(text)
            arg_count = sum(1 for line in text.splitlines()
                           if line.strip() and not line.strip().startswith("#"))
            result.append(PresetInfo(
                name=p.stem,
                description=meta.get("Description", ""),
                created=meta.get("Created", ""),
                modified=meta.get("Modified", ""),
                arg_count=arg_count,
                file_path=p,
            ))
        ZapretManager._preset_infos_cache = (signature, result)
        return result

    @staticmethod
    def read_preset(name: str) -> str:
        """Return full text content of a preset file."""
        path = PRESETS_DIR / f"{name}.txt"
        if not path.is_file():
            return ""
        return path.read_text(encoding="utf-8", errors="replace")

    @staticmethod
    def save_preset(name: str, content: str, description: str = "") -> PresetInfo:
        """Write preset file with updated metadata headers."""
        path = PRESETS_DIR / f"{name}.txt"
        PRESETS_DIR.mkdir(parents=True, exist_ok=True)

        # Preserve original Created date if file exists
        created = ""
        if path.is_file():
            old_text = path.read_text(encoding="utf-8", errors="replace")
            old_meta = ZapretManager._parse_metadata(old_text)
            created = old_meta.get("Created", "")

        now = datetime.now().isoformat(timespec="seconds")
        if not created:
            created = now

        # Strip existing metadata headers from content
        lines = content.splitlines()
        body_lines = []
        for line in lines:
            stripped = line.strip()
            if stripped.startswith("# Preset:") or stripped.startswith("# Description:") \
               or stripped.startswith("# Created:") or stripped.startswith("# Modified:"):
                continue
            body_lines.append(line)

        # Remove leading blank lines from body
        while body_lines and not body_lines[0].strip():
            body_lines.pop(0)

        header = f"# Preset: {name}\n# Description: {description}\n# Created: {created}\n# Modified: {now}\n\n"
        full_text = header + "\n".join(body_lines) + "\n"
        path.write_text(full_text, encoding="utf-8")
        ZapretManager.invalidate_preset_cache()

        arg_count = sum(1 for l in body_lines if l.strip() and not l.strip().startswith("#"))
        return PresetInfo(name=name, description=description, created=created,
                         modified=now, arg_count=arg_count, file_path=path)

    @staticmethod
    def rename_preset(old_name: str, new_name: str) -> PresetInfo | None:
        """Rename preset file. Returns new PresetInfo or None on failure."""
        old_path = PRESETS_DIR / f"{old_name}.txt"
        new_path = PRESETS_DIR / f"{new_name}.txt"
        if not old_path.is_file() or new_path.exists():
            return None

        # Update # Preset: header inside the file
        text = old_path.read_text(encoding="utf-8", errors="replace")
        text = text.replace(f"# Preset: {old_name}", f"# Preset: {new_name}", 1)
        new_path.write_text(text, encoding="utf-8")
        old_path.unlink()
        ZapretManager.invalidate_preset_cache()

        meta = ZapretManager._parse_metadata(text)
        arg_count = sum(1 for l in text.splitlines() if l.strip() and not l.strip().startswith("#"))
        return PresetInfo(name=new_name, description=meta.get("Description", ""),
                         created=meta.get("Created", ""), modified=meta.get("Modified", ""),
                         arg_count=arg_count, file_path=new_path)

    @staticmethod
    def delete_preset(name: str) -> bool:
        """Delete preset file. Returns True if deleted."""
        path = PRESETS_DIR / f"{name}.txt"
        if path.is_file():
            path.unlink()
            ZapretManager.invalidate_preset_cache()
            return True
        return False

    @staticmethod
    def import_preset(source_path: Path) -> PresetInfo | None:
        """Import a preset file from external path. Handles name conflicts."""
        if not source_path.is_file():
            return None
        PRESETS_DIR.mkdir(parents=True, exist_ok=True)

        base_name = source_path.stem
        target = PRESETS_DIR / f"{base_name}.txt"
        counter = 1
        while target.exists():
            target = PRESETS_DIR / f"{base_name} ({counter}).txt"
            counter += 1

        shutil.copy2(source_path, target)
        ZapretManager.invalidate_preset_cache()

        # Read and return info
        text = target.read_text(encoding="utf-8", errors="replace")
        meta = ZapretManager._parse_metadata(text)
        arg_count = sum(1 for l in text.splitlines() if l.strip() and not l.strip().startswith("#"))
        return PresetInfo(name=target.stem, description=meta.get("Description", ""),
                         created=meta.get("Created", ""), modified=meta.get("Modified", ""),
                         arg_count=arg_count, file_path=target)

    def _preflight_start(self, exe: Path, preset: Path, preset_name: str) -> str:
        if not preset_name:
            return "Не выбран пресет Zapret."
        if not exe.exists():
            return f"winws2.exe не найден: {exe}"
        if not preset.exists():
            return f"Пресет не найден: {preset}"
        exe_dir = exe.parent
        required = ("WinDivert.dll", "Monkey64.sys")
        missing = [name for name in required if not (exe_dir / name).exists()]
        if missing:
            return f"Не найдены файлы Zapret: {', '.join(missing)}"
        return ""

    @staticmethod
    def _referenced_zapret_files(args: list[str]) -> list[Path]:
        paths: list[Path] = []
        for arg in args:
            match = _LIST_FILE_ARG_RE.match(str(arg or "").strip())
            if not match:
                continue
            value = match.group(1).strip().strip('"')
            if not value or "," in value or "://" in value:
                continue
            path = Path(value)
            if path.is_absolute():
                continue
            paths.append(ZAPRET_DIR / path)
        return paths

    @staticmethod
    def _ensure_compatibility_lists() -> None:
        lists_dir = ZAPRET_DIR / "lists"
        ipset_base = lists_dir / "ipset-base.txt"
        ipset_all = lists_dir / "ipset-all.txt"
        if ipset_base.exists() or not ipset_all.is_file():
            return
        try:
            shutil.copy2(ipset_all, ipset_base)
        except OSError:
            pass

    @staticmethod
    def _missing_referenced_files(args: list[str]) -> list[Path]:
        ZapretManager._ensure_compatibility_lists()
        missing: list[Path] = []
        for path in ZapretManager._referenced_zapret_files(args):
            if not path.is_file():
                missing.append(path)
        return missing

    def start(
        self,
        preset_name: str,
        *,
        _retry_count: int = 0,
        _force_cleanup: bool = False,
        _launch_mode: str = "direct",
    ) -> None:
        preset_name = (preset_name or "").strip()
        exe = WINWS2_EXE
        preset = self.preset_path(preset_name)
        preflight_error = self._preflight_start(exe, preset, preset_name)
        if preflight_error:
            self.error.emit(preflight_error)
            return

        args = self._parse_preset_args(preset)
        if not args:
            self.error.emit(f"Пресет пустой: {preset_name}")
            return

        missing_files = self._missing_referenced_files(args)
        if missing_files:
            preview_items: list[str] = []
            for path in missing_files[:6]:
                try:
                    preview_items.append(str(path.relative_to(ZAPRET_DIR)))
                except ValueError:
                    preview_items.append(str(path))
            preview = ", ".join(preview_items)
            if len(missing_files) > 6:
                preview += f" ... (+{len(missing_files) - 6})"
            self.error.emit(f"Не найдены файлы списков Zapret: {preview}")
            return

        was_running = self.running
        if self.running:
            if self._current_preset == preset_name:
                self.log_line.emit(f"[zapret] Перезапуск текущего пресета: {preset_name}")
            else:
                self.log_line.emit(f"[zapret] Переключение пресета: {self._current_preset} -> {preset_name}")
            self.stop(fast=True, cleanup=False, emit_stopped=False)

        killed = [] if was_running and not _force_cleanup else self._kill_orphaned(
            timeout=0.8 if not _force_cleanup else 5,
            taskkill_timeout=0.8 if not _force_cleanup else 3,
            settle_delay=0.1 if not _force_cleanup else 1.25,
        )
        for name in killed:
            self.log_line.emit(f"[zapret] Завершён сторонний процесс: {name}")

        if _force_cleanup:
            self.log_line.emit("[zapret] WinDivert конфликт: выполняется глубокая очистка перед повторным запуском")
            self._cleanup_windivert(timeout=2.5)
            sleep_with_events(0.15)
        elif not was_running:
            self._standard_windivert_cleanup()
        else:
            sleep_with_events(0.05)

        launch_mode = "at_config" if _launch_mode == "at_config" else "direct"
        at_config: Path | None = None
        start_args = list(args)
        if launch_mode == "at_config":
            at_config = self._write_at_config(preset, args)
            start_args = [f"@{at_config}"]

        self._current_preset = preset_name
        self._start_args = start_args
        self._start_retry_count = int(_retry_count)
        self._current_at_config = at_config
        self._launch_mode = launch_mode
        self._output_tail = []

        self._process = QProcess(self)
        self._process.setProgram(str(exe))
        self._process.setArguments(self._start_args)
        self._process.setWorkingDirectory(str(ZAPRET_DIR))
        self._process.readyReadStandardOutput.connect(self._on_stdout)
        self._process.readyReadStandardError.connect(self._on_stderr)
        self._process.finished.connect(self._on_finished)

        if at_config is not None:
            log.info("zapret start: %s [%s] via %s (%d args)", exe.name, preset_name, at_config.name, len(args))
            self.log_line.emit(f"[zapret] Запуск: {preset_name} ({len(args)} аргументов через @{at_config.name})")
        else:
            log.info("zapret start: %s [%s] via direct argv (%d args)", exe.name, preset_name, len(args))
            self.log_line.emit(f"[zapret] Запуск: {preset_name} ({len(args)} аргументов напрямую)")
        self._process.start()

        if not wait_for_qprocess_started(self._process, 1200):
            launch_error = self._process.errorString()
            if _is_elevation_launch_error(launch_error):
                self.log_line.emit(f"[zapret] Запуск winws2.exe отклонён Windows: {launch_error}")
                self.error.emit("Для запуска Zapret нужны права администратора. Перезапустите Lumen KVN от имени администратора.")
                self._process = None
                self._launch_mode = "direct"
                return
            if launch_mode == "direct":
                preset_name = self._current_preset
                self.log_line.emit("[zapret] Прямой запуск не стартовал, пробую fallback через @config из ProgramData")
                self._process = None
                QTimer.singleShot(150, lambda name=preset_name: self.start(
                    name,
                    _retry_count=2,
                    _force_cleanup=False,
                    _launch_mode="at_config",
                ))
                return
            self.error.emit(f"Не удалось запустить winws2.exe: {launch_error}")
            self._process = None
            self._launch_mode = "direct"
            return

        self._health_timer.start()
        self.started.emit()

    def stop(self, *, fast: bool = False, cleanup: bool = True, emit_stopped: bool = True) -> None:
        self._health_timer.stop()
        process = self._process
        if process is None:
            return

        try:
            process.finished.disconnect(self._on_finished)
        except (TypeError, RuntimeError):
            pass

        if process.state() == QProcess.ProcessState.Running:
            log.info("zapret stop")
            process.terminate()
            terminate_timeout = 600 if fast else 1800
            kill_timeout = 800 if fast else 5000
            if not wait_for_qprocess_finished(process, terminate_timeout):
                process.kill()
                wait_for_qprocess_finished(process, kill_timeout)

        self._process = None
        self._current_preset = ""
        self._start_args = []
        self._start_retry_count = 0
        self._current_at_config = None
        self._launch_mode = "direct"
        self._output_tail = []
        killed = self._kill_orphaned(
            timeout=1 if fast else 5,
            taskkill_timeout=1 if fast else 3,
            settle_delay=0 if fast else 1.25,
        )
        for name in killed:
            self.log_line.emit(f"[zapret] Завершён оставшийся процесс: {name}")
        if cleanup:
            self._standard_windivert_cleanup()
        if cleanup and not fast:
            sleep_with_events(0.5)
        if emit_stopped:
            self.stopped.emit()

    # ── internals ───────────────────────────────────────────────

    @staticmethod
    def _kill_orphaned(
        *, timeout: float = 5.0, taskkill_timeout: float = 3.0, settle_delay: float = 1.25
    ) -> list[str]:
        """Kill any orphaned winws.exe / winws2.exe processes."""
        killed: list[str] = []
        if os.name != "nt":
            return killed
        for exe_name, exe_path in (("winws2.exe", WINWS2_EXE), ("winws.exe", WINWS_EXE)):
            try:
                if kill_processes_by_path(exe_name, exe_path, timeout=timeout):
                    killed.append(exe_name)
            except Exception:
                pass
            try:
                result = run_text_pumped(
                    ["taskkill", "/F", "/T", "/IM", exe_name],
                    timeout=taskkill_timeout,
                    creationflags=CREATE_NO_WINDOW,
                )
                if result.returncode == 0 and exe_name not in killed:
                    killed.append(exe_name)
            except Exception:
                pass
        if killed:
            sleep_with_events(settle_delay)
        return killed

    @staticmethod
    def _find_windivert_services() -> list[str]:
        if os.name != "nt":
            return []
        if ZapretManager._windivert_services_cache is not None:
            return list(ZapretManager._windivert_services_cache)
        found: set[str] = {"WinDivert", "WinDivert14", "WinDivert64", "WinDivert2", "Monkey"}
        try:
            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Services") as root:
                index = 0
                while True:
                    try:
                        name = winreg.EnumKey(root, index)
                    except OSError:
                        break
                    index += 1
                    try:
                        with winreg.OpenKey(root, name) as service_key:
                            image_path, _ = winreg.QueryValueEx(service_key, "ImagePath")
                    except OSError:
                        continue
                    image_text = str(image_path or "").lower()
                    if any(marker in image_text for marker in ("monkey64", "monkey32", "windivert")):
                        found.add(name)
        except OSError:
            pass
        services = sorted(found)
        ZapretManager._windivert_services_cache = services
        return list(services)

    def _standard_windivert_cleanup(self, *, force: bool = False) -> None:
        if os.name != "nt":
            return
        if ZapretManager._standard_cleanup_done and not force:
            return
        for service in self._find_windivert_services():
            try:
                run_text_pumped(
                    ["sc", "config", service, "start=", "demand"],
                    timeout=0.8,
                    creationflags=CREATE_NO_WINDOW,
                )
            except Exception:
                pass
        ZapretManager._standard_cleanup_done = True
        sleep_with_events(0.05 if not force else 0.2)

    def _cleanup_windivert(self, *, timeout: float = 3.0) -> None:
        if os.name != "nt":
            return
        ZapretManager._windivert_services_cache = None
        ZapretManager._standard_cleanup_done = False
        for service in self._find_windivert_services():
            for action in ("stop", "delete"):
                try:
                    run_text_pumped(
                        ["sc", action, service],
                        timeout=timeout,
                        creationflags=CREATE_NO_WINDOW,
                    )
                except Exception:
                    pass

    @staticmethod
    def _looks_like_windivert_conflict(exit_code: int, lines: list[str]) -> bool:
        if exit_code != 1:
            return False
        text = "\n".join(lines).lower()
        if not text:
            return True
        return any(marker in text for marker in _WINDIVERT_CONFLICT_MARKERS)

    @staticmethod
    def _exit_code_hint(code: int) -> str:
        """Return a human-readable hint for common winws2 exit codes."""
        hints = {
            1: "общая ошибка (другой экземпляр / не удалось открыть WinDivert)",
            2: "ошибка аргументов командной строки",
            3: "не удалось загрузить WinDivert драйвер (нужны права администратора)",
        }
        return hints.get(code, "")

    def _drain_output(self) -> list[str]:
        """Read any remaining stdout/stderr from the process."""
        lines: list[str] = []
        if self._process is None:
            return lines
        for reader in (self._process.readAllStandardOutput,
                       self._process.readAllStandardError):
            data = reader().data()
            if data:
                for line in decode_output(bytes(data)).splitlines():
                    stripped = line.strip()
                    if stripped:
                        lines.append(stripped)
        return lines

    def _on_stdout(self) -> None:
        if self._process is None:
            return
        data = self._process.readAllStandardOutput().data()
        for line in decode_output(bytes(data)).splitlines():
            if line.strip():
                stripped = line.strip()
                self._remember_output(stripped)
                self.log_line.emit(f"[zapret] {stripped}")

    def _on_stderr(self) -> None:
        if self._process is None:
            return
        data = self._process.readAllStandardError().data()
        for line in decode_output(bytes(data)).splitlines():
            if line.strip():
                stripped = line.strip()
                self._remember_output(stripped)
                self.log_line.emit(f"[zapret] {stripped}")

    def _remember_output(self, line: str) -> None:
        self._output_tail.append(line)
        if len(self._output_tail) > 80:
            del self._output_tail[:-80]

    def _on_finished(self, exit_code: int, exit_status: QProcess.ExitStatus) -> None:
        self._health_timer.stop()

        # Drain any buffered output before dropping the process reference
        remaining = self._drain_output()
        for line in remaining:
            self._remember_output(line)
            self.log_line.emit(f"[zapret] {line}")
        output_for_diagnostics = list(self._output_tail)

        preset = self._current_preset or "?"
        launch_mode = self._launch_mode
        log.info("zapret finished: code=%d status=%s preset=%s", exit_code, exit_status.name, preset)

        if exit_code != 0 or exit_status == QProcess.ExitStatus.CrashExit:
            if (
                launch_mode == "direct"
                and self._start_retry_count == 0
                and self._current_preset
                and exit_code == 2
            ):
                preset_name = self._current_preset
                self.log_line.emit("[zapret] Ошибка аргументов, пробую fallback через @config из ProgramData")
                self._process = None
                QTimer.singleShot(150, lambda name=preset_name: self.start(
                    name,
                    _retry_count=2,
                    _force_cleanup=False,
                    _launch_mode="at_config",
                ))
                return

            if (
                self._start_retry_count < 1
                and self._current_preset
                and self._looks_like_windivert_conflict(exit_code, output_for_diagnostics)
            ):
                preset_name = self._current_preset
                self.log_line.emit("[zapret] winws2 завершился с кодом 1, пробую повторный запуск после очистки WinDivert")
                self._process = None
                QTimer.singleShot(150, lambda name=preset_name: self.start(
                    name,
                    _retry_count=1,
                    _force_cleanup=True,
                    _launch_mode="direct",
                ))
                return

            if (
                launch_mode == "direct"
                and self._start_retry_count == 1
                and self._current_preset
                and exit_code in (1, 2)
            ):
                preset_name = self._current_preset
                self.log_line.emit("[zapret] Прямой запуск не прошёл, пробую fallback через @config из ProgramData")
                self._process = None
                QTimer.singleShot(150, lambda name=preset_name: self.start(
                    name,
                    _retry_count=2,
                    _force_cleanup=False,
                    _launch_mode="at_config",
                ))
                return

            # Подробности в лог
            hint = self._exit_code_hint(exit_code)
            if hint:
                self.log_line.emit(f"[zapret] Код {exit_code}: {hint}")
            self.log_line.emit(f"[zapret] Пресет: {preset}")
            if self._start_args:
                preview = " ".join(self._start_args[:6])
                if len(self._start_args) > 6:
                    preview += f" ... (+{len(self._start_args) - 6} аргументов)"
                self.log_line.emit(f"[zapret] Команда: winws2.exe {preview}")
            if not output_for_diagnostics:
                self.log_line.emit("[zapret] Процесс не вывел ничего в stdout/stderr")

            # Краткое сообщение для InfoBar и status_label
            short = f"winws2 завершился с кодом {exit_code}"
            if hint:
                short += f" — {hint}"
            self.error.emit(short)

        self._process = None
        self._current_preset = ""
        self._start_args = []
        self._start_retry_count = 0
        self._current_at_config = None
        self._launch_mode = "direct"
        self._output_tail = []
        self.stopped.emit()

    def _check_health(self) -> None:
        if not self.running:
            self._health_timer.stop()
            self.stopped.emit()
