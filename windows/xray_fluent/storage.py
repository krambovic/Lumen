from __future__ import annotations

from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil

from .constants import (
    CONFIGS_DIR,
    DATA_DIR,
    LOG_DIR,
    RUNTIME_DIR,
    SINGBOX_CONFIGS_DIR,
    SINGBOX_PATH_DEFAULT,
    SINGBOX_TEMPLATES_DIR,
    STATE_FILE,
    XRAY_CONFIGS_DIR,
    XRAY_TEMPLATES_DIR,
    XRAY_PATH_DEFAULT,
)
from .models import AppState
from .path_utils import normalize_configured_path
from .security import (
    decode_encrypted,
    decrypt_with_passphrase,
    encode_encrypted,
    encrypt_with_passphrase,
    is_passphrase_encrypted,
)


class PassphraseRequired(Exception):
    pass


class StateLoadError(Exception):
    pass


class StateStorage:
    _BACKUP_COUNT = 3
    _PATH_MIGRATION_MARKER = ".state_path_migration_v1"

    def __init__(self, state_file: Path = STATE_FILE):
        self.state_file = state_file
        self._passphrase: str = ""
        self._ensure_dirs()

    @property
    def passphrase(self) -> str:
        return self._passphrase

    @passphrase.setter
    def passphrase(self, value: str) -> None:
        self._passphrase = value

    def _ensure_dirs(self) -> None:
        DATA_DIR.mkdir(parents=True, exist_ok=True)
        CONFIGS_DIR.mkdir(parents=True, exist_ok=True)
        SINGBOX_TEMPLATES_DIR.mkdir(parents=True, exist_ok=True)
        XRAY_TEMPLATES_DIR.mkdir(parents=True, exist_ok=True)
        SINGBOX_CONFIGS_DIR.mkdir(parents=True, exist_ok=True)
        XRAY_CONFIGS_DIR.mkdir(parents=True, exist_ok=True)
        RUNTIME_DIR.mkdir(parents=True, exist_ok=True)
        LOG_DIR.mkdir(parents=True, exist_ok=True)

    def _default_state(self) -> AppState:
        state = AppState()
        try:
            from .routing_presets import ROUTING_PRESET_BLOCKED, build_routing_preset
            state.routing = build_routing_preset(state.routing, ROUTING_PRESET_BLOCKED)
        except Exception:
            pass
        return self._normalize_state_paths(state)

    def default_state(self) -> AppState:
        """Return a fresh default state with immediately usable core paths."""
        return self._default_state()

    def _normalize_state_paths(self, state: AppState) -> AppState:
        state.settings.xray_path = normalize_configured_path(
            state.settings.xray_path,
            default_path=XRAY_PATH_DEFAULT,
            use_default_if_empty=True,
            migrate_default_location=True,
        )
        state.settings.singbox_path = normalize_configured_path(
            state.settings.singbox_path,
            default_path=SINGBOX_PATH_DEFAULT,
            use_default_if_empty=True,
            migrate_default_location=True,
        )
        state.settings.xray_config_file = self._normalize_profile_reference(
            state.settings.xray_config_file,
            XRAY_CONFIGS_DIR,
        )
        state.settings.xray_template_file = self._normalize_profile_reference(
            state.settings.xray_template_file,
            XRAY_TEMPLATES_DIR,
        )
        state.settings.singbox_config_file = self._normalize_profile_reference(
            state.settings.singbox_config_file,
            SINGBOX_CONFIGS_DIR,
        )
        state.settings.singbox_template_file = self._normalize_profile_reference(
            state.settings.singbox_template_file,
            SINGBOX_TEMPLATES_DIR,
        )
        return state

    @staticmethod
    def _normalize_profile_reference(value: str | Path | None, base_dir: Path) -> str:
        text = str(value or "").strip()
        if not text:
            return ""

        candidate = Path(text)
        if not candidate.is_absolute():
            return candidate.as_posix()

        resolved_base = base_dir.resolve(strict=False)
        resolved_candidate = candidate.resolve(strict=False)
        try:
            return resolved_candidate.relative_to(resolved_base).as_posix()
        except ValueError:
            pass

        # Older builds stored full paths below data/configs or data/templates.
        # Recover the relative suffix after the matching profile directory so a
        # verified data-directory migration cannot leave the state pointing at
        # the removed Lumen KVN tree.
        base_tail = tuple(part.casefold() for part in base_dir.parts[-2:])
        candidate_parts = candidate.parts
        candidate_folded = tuple(part.casefold() for part in candidate_parts)
        legacy_names = {"lumen kvn", "lumenkvn", "lumen-kvn", "lumen_kvn"}
        contains_legacy_name = any(part in legacy_names for part in candidate_folded)
        for index in range(len(candidate_parts) - len(base_tail), -1, -1):
            if candidate_folded[index:index + len(base_tail)] != base_tail:
                continue
            suffix = candidate_parts[index + len(base_tail):]
            if not suffix or any(part in ("", ".", "..") for part in suffix):
                continue
            migrated = resolved_base.joinpath(*suffix)
            if contains_legacy_name or migrated.is_file():
                return Path(*suffix).as_posix()

        fallback = resolved_base / candidate.name
        if candidate.name and fallback.is_file():
            return candidate.name
        return str(candidate)

    @staticmethod
    def _state_path_values(state: AppState) -> tuple[str, ...]:
        settings = state.settings
        return (
            settings.xray_path,
            settings.singbox_path,
            settings.xray_config_file,
            settings.xray_template_file,
            settings.singbox_config_file,
            settings.singbox_template_file,
        )

    def _serialize_state(self, state: AppState) -> str:
        payload = state.to_dict()
        settings_payload = dict(payload.get("settings") or {})
        settings_payload["xray_path"] = normalize_configured_path(
            settings_payload.get("xray_path"),
            default_path=XRAY_PATH_DEFAULT,
            use_default_if_empty=True,
            migrate_default_location=True,
        )
        settings_payload["singbox_path"] = normalize_configured_path(
            settings_payload.get("singbox_path"),
            default_path=SINGBOX_PATH_DEFAULT,
            use_default_if_empty=True,
            migrate_default_location=True,
        )
        settings_payload["xray_config_file"] = self._normalize_profile_reference(
            settings_payload.get("xray_config_file"),
            XRAY_CONFIGS_DIR,
        )
        settings_payload["xray_template_file"] = self._normalize_profile_reference(
            settings_payload.get("xray_template_file"),
            XRAY_TEMPLATES_DIR,
        )
        settings_payload["singbox_config_file"] = self._normalize_profile_reference(
            settings_payload.get("singbox_config_file"),
            SINGBOX_CONFIGS_DIR,
        )
        settings_payload["singbox_template_file"] = self._normalize_profile_reference(
            settings_payload.get("singbox_template_file"),
            SINGBOX_TEMPLATES_DIR,
        )
        payload["settings"] = settings_payload
        return json.dumps(payload, ensure_ascii=True, indent=2)

    def _quarantine_unreadable_state(self) -> Path | None:
        if not self.state_file.exists():
            return None
        stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
        quarantine = self.state_file.with_name(f"{self.state_file.name}.corrupt-{stamp}")
        try:
            self.state_file.replace(quarantine)
            return quarantine
        except Exception:
            return None

    def _backup_path(self, index: int) -> Path:
        return self.state_file.with_name(f"{self.state_file.name}.bak{index}")

    def _tmp_path(self) -> Path:
        return self.state_file.with_name(f".{self.state_file.name}.tmp")

    def _decode_state_details(self, raw_text: str) -> tuple[AppState, bool, str]:
        raw_text = raw_text.strip()
        if not raw_text:
            raise StateLoadError("state file is empty")

        if is_passphrase_encrypted(raw_text):
            if not self._passphrase:
                raise PassphraseRequired()
            decrypted = decrypt_with_passphrase(raw_text, self._passphrase)
            payload = json.loads(decrypted.decode("utf-8"))
            encoding = "passphrase"
        elif raw_text.startswith("{"):
            payload = json.loads(raw_text)
            encoding = "plain"
        else:
            try:
                payload = json.loads(decode_encrypted(raw_text).decode("utf-8"))
                encoding = "dpapi"
            except Exception:
                payload = json.loads(raw_text)
                encoding = "plain"
        state = AppState.from_dict(payload)
        before = self._state_path_values(state)
        normalized = self._normalize_state_paths(state)
        return normalized, self._state_path_values(normalized) != before, encoding

    def _decode_state(self, raw_text: str) -> AppState:
        return self._decode_state_details(raw_text)[0]

    def _encode_state(self, state: AppState, encoding: str) -> str:
        payload = self._serialize_state(state).encode("utf-8")
        if encoding == "passphrase":
            return encrypt_with_passphrase(payload, self._passphrase)
        if encoding == "dpapi":
            return encode_encrypted(payload)
        return payload.decode("utf-8")

    def _atomic_write_path(self, path: Path, content: str) -> None:
        staged = path.with_name(f".{path.name}.migration.tmp")
        with open(staged, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(staged, path)
        self._fsync_directory(path.parent)

    def _normalize_backup_paths(self) -> bool:
        complete = True
        for index in range(1, self._BACKUP_COUNT + 1):
            backup = self._backup_path(index)
            if not backup.is_file():
                continue
            try:
                raw_text = backup.read_text(encoding="utf-8")
                state, changed, encoding = self._decode_state_details(raw_text)
                if changed:
                    self._atomic_write_path(backup, self._encode_state(state, encoding))
            except Exception:
                # A damaged backup must not prevent the valid primary state
                # from loading. The normal recovery path will quarantine it if
                # it is ever needed.
                complete = False
        return complete

    def _normalize_backup_paths_once(self) -> None:
        marker = self.state_file.with_name(self._PATH_MIGRATION_MARKER)
        if marker.is_file():
            return
        if not self._normalize_backup_paths():
            return
        try:
            self._atomic_write_path(marker, "complete\n")
        except OSError:
            pass

    @staticmethod
    def _fsync_directory(path: Path) -> None:
        if os.name == "nt":
            return
        try:
            descriptor = os.open(path, os.O_RDONLY)
        except OSError:
            return
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)

    def _atomic_write_content(self, content: str) -> None:
        tmp_file = self._tmp_path()
        with open(tmp_file, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp_file, self.state_file)
        self._fsync_directory(self.state_file.parent)

    def _rotate_backups(self) -> None:
        if not self.state_file.is_file() or self.state_file.stat().st_size <= 0:
            return
        for index in range(self._BACKUP_COUNT, 1, -1):
            previous = self._backup_path(index - 1)
            current = self._backup_path(index)
            if previous.is_file():
                os.replace(previous, current)
        backup = self._backup_path(1)
        staged = backup.with_name(f".{backup.name}.tmp")
        shutil.copy2(self.state_file, staged)
        os.replace(staged, backup)

    def is_encrypted(self) -> bool:
        if not self.state_file.exists():
            return False
        raw = self.state_file.read_text(encoding="utf-8").strip()
        return is_passphrase_encrypted(raw)

    def load(self) -> AppState:
        self._ensure_dirs()
        candidates = [
            self.state_file,
            self._tmp_path(),
            *(self._backup_path(index) for index in range(1, self._BACKUP_COUNT + 1)),
        ]
        existing = [path for path in candidates if path.is_file()]
        if not existing:
            return self._default_state()

        errors: list[str] = []
        for candidate in existing:
            try:
                raw_text = candidate.read_text(encoding="utf-8")
                state, paths_migrated, encoding = self._decode_state_details(raw_text)
            except PassphraseRequired:
                if candidate == self.state_file:
                    raise
                continue
            except Exception as exc:
                errors.append(f"{candidate.name}: {exc}")
                continue

            if candidate != self.state_file:
                self._quarantine_unreadable_state()
                restored_content = (
                    self._encode_state(state, encoding) if paths_migrated else raw_text.strip()
                )
                self._atomic_write_content(restored_content)
            elif paths_migrated:
                self._atomic_write_content(self._encode_state(state, encoding))
            # Backups may predate a successfully normalized primary file. Scan
            # them independently so an older absolute product path cannot be
            # restored by a later recovery.
            self._normalize_backup_paths_once()
            return state

        quarantine = self._quarantine_unreadable_state()
        suffix = f" Файл сохранён как {quarantine}." if quarantine else ""
        detail = "; ".join(errors[:4])
        raise StateLoadError(
            f"Не удалось восстановить {self.state_file.name}.{suffix} {detail}".strip()
        )

    def save(self, state: AppState) -> None:
        self._ensure_dirs()
        payload = self._serialize_state(state)

        if self._passphrase:
            content = encrypt_with_passphrase(payload.encode("utf-8"), self._passphrase)
        else:
            content = payload

        self._rotate_backups()
        self._atomic_write_content(content)

    def export_backup(self, path: Path, passphrase: str = "") -> None:
        state = self.load()
        payload = self._serialize_state(state)
        if passphrase:
            content = encrypt_with_passphrase(payload.encode("utf-8"), passphrase)
        else:
            content = payload
        path.write_text(content, encoding="utf-8")

    def import_backup(self, path: Path, passphrase: str = "") -> AppState:
        raw = path.read_text(encoding="utf-8").strip()
        if is_passphrase_encrypted(raw):
            if not passphrase:
                raise PassphraseRequired()
            decrypted = decrypt_with_passphrase(raw, passphrase)
            payload = json.loads(decrypted.decode("utf-8"))
        else:
            payload = json.loads(raw)
        return self._normalize_state_paths(AppState.from_dict(payload))
