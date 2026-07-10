from __future__ import annotations

import pytest

from xray_fluent.core_resource_updater import ResourceUpdateWorker, _atomic_replace_files
from xray_fluent.engines.xray.core_updater import UpdateCancelled, XrayCoreUpdateWorker


def test_atomic_replace_rolls_every_file_back_when_validation_fails(tmp_path) -> None:
    first = tmp_path / "first.dat"
    second = tmp_path / "second.dat"
    first.write_bytes(b"old-first")
    second.write_bytes(b"old-second")
    new_first = tmp_path / "new-first.dat"
    new_second = tmp_path / "new-second.dat"
    new_first.write_bytes(b"new-first")
    new_second.write_bytes(b"new-second")

    def reject_install() -> None:
        raise RuntimeError("invalid install")

    with pytest.raises(RuntimeError, match="invalid install"):
        _atomic_replace_files(
            [(new_first, first), (new_second, second)],
            validator=reject_install,
        )

    assert first.read_bytes() == b"old-first"
    assert second.read_bytes() == b"old-second"
    assert not list(tmp_path.glob("*.rollback"))
    assert not list(tmp_path.glob("*.new"))


def test_atomic_replace_writes_backup_only_after_success(tmp_path) -> None:
    target = tmp_path / "sing-box.exe"
    source = tmp_path / "downloaded.exe"
    target.write_bytes(b"old")
    source.write_bytes(b"new")

    _atomic_replace_files([(source, target)], backup_targets={target})

    assert target.read_bytes() == b"new"
    assert target.with_suffix(".exe.bak").read_bytes() == b"old"


@pytest.mark.parametrize(
    "worker",
    [
        lambda: ResourceUpdateWorker("geodata"),
        lambda: XrayCoreUpdateWorker("xray.exe", "stable", "", True),
    ],
)
def test_updater_cancel_unblocks_disconnect_handshake(worker) -> None:
    worker = worker()
    worker.cancel()

    with pytest.raises(UpdateCancelled):
        worker._trigger_disconnect_request()


def test_resource_worker_cancel_closes_active_response() -> None:
    class _Response:
        closed = False

        def close(self) -> None:
            self.closed = True

    worker = ResourceUpdateWorker("geodata")
    response = _Response()
    worker._register_response(response)

    worker.cancel()

    assert response.closed is True
