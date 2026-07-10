from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.qml_app.bridge.app_bridge import AppBridge


class _Signal:
    def __init__(self) -> None:
        self.count = 0
        self.args: list[tuple] = []

    def emit(self, *args) -> None:
        self.count += 1
        self.args.append(args)


class _Worker:
    def __init__(self) -> None:
        self.stopped = False

    def stop(self) -> None:
        self.stopped = True


class _Controller:
    def __init__(self) -> None:
        self.applied = []

    def apply_fetched_subscription(self, *args):
        self.applied.append(args)
        return 1, []


def test_cancel_subscription_import_stops_worker_and_clears_state() -> None:
    worker = _Worker()
    bridge = SimpleNamespace(
        _sub_importing=True,
        _sub_import_status="Downloading...",
        _sub_batches={1: {"kind": "import", "added": 0, "errors": []}},
        _sub_worker=worker,
        _sub_thread=None,
        subscriptionImportingChanged=_Signal(),
        subscriptionImportStatusChanged=_Signal(),
        toast=_Signal(),
    )

    AppBridge.cancelSubscriptionImport(bridge)

    assert bridge._sub_importing is False
    assert bridge._sub_import_status == ""
    assert bridge._sub_batches == {}
    assert worker.stopped is True
    assert bridge.subscriptionImportingChanged.count == 1
    assert bridge.subscriptionImportStatusChanged.count == 1
    assert bridge.toast.args


def test_cancelled_subscription_import_result_is_ignored() -> None:
    controller = _Controller()
    bridge = SimpleNamespace(
        controller=controller,
        _sub_batches={},
    )
    job = SimpleNamespace(url="https://sub.example", name="Sub", kind="import")

    AppBridge._on_sub_fetched(bridge, 7, job, "vless://...", {}, [])

    assert controller.applied == []
