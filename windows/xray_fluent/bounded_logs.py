"""Bounded, thread-safe UI log history with snapshot iteration.

LogRing(maxlen=5000, iterable=()) drops the oldest entry on overflow. ``dropped``
is cumulative; clear/drain remove entries without resetting this counter.
"""
from __future__ import annotations

from collections import deque
from collections.abc import Iterable, Iterator, Sequence
from threading import RLock
from typing import Generic, TypeVar, overload

T = TypeVar("T")


class LogRing(Sequence[T], Generic[T]):
    def __init__(self, maxlen: int = 5000, iterable: Iterable[T] = ()) -> None:
        if type(maxlen) is not int or maxlen < 1:
            raise ValueError("maxlen must be a positive integer")
        self._items: deque[T] = deque(maxlen=maxlen)
        self._lock = RLock()
        self._dropped = 0
        self.extend(iterable)

    @property
    def maxlen(self) -> int:
        return self._items.maxlen  # type: ignore[return-value]

    @property
    def capacity(self) -> int:
        return self.maxlen

    @property
    def dropped(self) -> int:
        with self._lock:
            return self._dropped

    @property
    def dropped_count(self) -> int:
        return self.dropped

    def append(self, item: T) -> None:
        with self._lock:
            if len(self._items) == self.maxlen:
                self._dropped += 1
            self._items.append(item)

    def extend(self, items: Iterable[T]) -> None:
        # Never invoke an application iterator while holding the ring lock.
        for item in items:
            self.append(item)

    def __len__(self) -> int:
        with self._lock:
            return len(self._items)

    def __iter__(self) -> Iterator[T]:
        with self._lock:
            return iter(tuple(self._items))

    @overload
    def __getitem__(self, index: int) -> T: ...

    @overload
    def __getitem__(self, index: slice) -> list[T]: ...

    def __getitem__(self, index: int | slice) -> T | list[T]:
        with self._lock:
            if isinstance(index, slice):
                return list(self._items)[index]
            return self._items[index]

    def clear(self) -> None:
        with self._lock:
            self._items.clear()

    def drain(self, limit: int | None = None) -> list[T]:
        """Atomically remove and return up to ``limit`` oldest entries."""
        if limit is not None and (type(limit) is not int or limit < 0):
            raise ValueError("limit must be a nonnegative integer or None")
        with self._lock:
            count = len(self._items) if limit is None else min(limit, len(self._items))
            return [self._items.popleft() for _ in range(count)]
