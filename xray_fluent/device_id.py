"""Stable per-installation device identifier (HWID) for HAPP subscriptions.

Many HAPP providers bind a subscription to a hardware id and/or enforce a
"max devices" limit. Both features rely on the client sending a **stable,
unique** ``X-Hwid`` header on every subscription request:

* If the value keeps changing, a device-limited panel counts each refresh as a
  brand-new device and quickly locks the user out.
* If every install sends the *same* value (Lumen historically shipped a
  hard-coded all-zero id), the panel treats every Lumen user as one device, so
  HWID binding and per-device limits cannot work at all.

So Lumen generates a random UUID once, persists it next to the app state, and
reuses it forever. That mirrors what the Happ client itself does with its own
device id.
"""

from __future__ import annotations

import threading
import uuid

from .constants import DATA_DIR

_HWID_FILE = DATA_DIR / "device.id"
_lock = threading.Lock()
_cached_hwid: str | None = None


def _is_valid_hwid(value: str) -> bool:
    try:
        uuid.UUID(value)
    except (ValueError, AttributeError, TypeError):
        return False
    return True


def get_device_hwid() -> str:
    """Return this installation's stable HWID, creating it on first use.

    The value is a lowercase UUID string. Reads/writes are cached in memory and
    guarded by a lock so it is safe to call from the subscription worker thread.
    """
    global _cached_hwid
    if _cached_hwid is not None:
        return _cached_hwid
    with _lock:
        if _cached_hwid is not None:
            return _cached_hwid
        hwid = ""
        try:
            if _HWID_FILE.exists():
                hwid = _HWID_FILE.read_text(encoding="utf-8").strip()
        except Exception:  # noqa: BLE001 - unreadable file → regenerate
            hwid = ""
        if not _is_valid_hwid(hwid):
            hwid = str(uuid.uuid4())
            try:
                _HWID_FILE.parent.mkdir(parents=True, exist_ok=True)
                _HWID_FILE.write_text(hwid, encoding="utf-8")
            except Exception:  # noqa: BLE001 - non-fatal: fall back to volatile id
                pass
        _cached_hwid = hwid
        return hwid
