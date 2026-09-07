"""Owned snapshots: capture on the state owner, decode on the disk writer."""
from copy import copy
from dataclasses import dataclass
import json
from .models import AppSettings, AppState


@dataclass(frozen=True, slots=True)
class StateSnapshot:
    payload: str
    settings: AppSettings

    def to_dict(self) -> dict:
        return json.loads(self.payload)


def snapshot_state(state: AppState) -> StateSnapshot:
    # The C JSON encoder avoids recursive Python deepcopy of each history
    # tuple and dataclass. No mutable config/history references cross threads.
    # The owner must call this before yielding to another state mutation.
    payload = json.dumps(state.to_dict(), ensure_ascii=True, separators=(",", ":"))
    return StateSnapshot(payload, copy(state.settings))
