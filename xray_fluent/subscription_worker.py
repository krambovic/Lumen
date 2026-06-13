"""Фоновая загрузка подписок, чтобы UI-поток не фризил на сетевых запросах.

Сеть идёт в отдельном QThread; применение результата к state делается в GUI-потоке.
"""
from __future__ import annotations

from dataclasses import dataclass

from PyQt6.QtCore import QObject, pyqtSignal

from .application.node_service import fetch_subscription_payload


@dataclass
class SubscriptionJob:
    """Одна задача загрузки подписки."""

    url: str
    kind: str  # "import" | "update"
    name: str = ""


class SubscriptionFetchWorker(QObject):
    """Грузит подписки по сети в фоновом потоке."""

    # batch_id, job, text, userinfo, errors
    fetched = pyqtSignal(int, object, str, object, object)
    # batch_id, total
    completed = pyqtSignal(int, int)

    def run_batch(self, jobs: object, batch_id: int) -> None:
        total = 0
        for job in list(jobs or []):
            try:
                text, userinfo, errors = fetch_subscription_payload(job.url)
            except Exception as exc:  # никогда не роняем рабочий поток
                text, userinfo, errors = "", {}, [str(exc)]
            self.fetched.emit(batch_id, job, text, userinfo, list(errors))
            total += 1
        self.completed.emit(batch_id, total)
