# Process-wide route ownership shared by ping, speed and direct HTTP.
from dataclasses import dataclass
import threading
from typing import Callable

@dataclass
class RouteLease:
    refs: int
    owned: bool
    delete: Callable[[], None]

_lock = threading.RLock()
_leases: dict[tuple, RouteLease] = {}


def acquire_route(key: tuple, create, exists, delete) -> bool:
    with _lock:
        lease = _leases.get(key)
        if lease is not None:
            lease.refs += 1
            return True
        if exists():
            owned = False
        else:
            owned = bool(create())
            if not owned and not exists():
                return False
        _leases[key] = RouteLease(1, owned, delete)
        return True


def release_route(key: tuple) -> None:
    with _lock:
        lease = _leases.get(key)
        if lease is None:
            return
        lease.refs -= 1
        if lease.refs > 0:
            return
        # Keep the lock through deletion so a new lease cannot race cleanup.
        try:
            if lease.owned:
                lease.delete()
        finally:
            # A failed OS cleanup must not leave a zero-reference lease that
            # makes later callers believe the route is still owned/usable.
            _leases.pop(key, None)
