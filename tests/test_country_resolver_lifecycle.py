from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.application import node_runtime_service


def test_country_resolution_does_not_replace_running_worker(monkeypatch) -> None:
    class _RunningWorker:
        def isRunning(self) -> bool:
            return True

    def unexpected_resolver(*_args, **_kwargs):
        raise AssertionError("a second country resolver must not be created")

    monkeypatch.setattr(node_runtime_service, "CountryResolver", unexpected_resolver)
    controller = SimpleNamespace(
        state=SimpleNamespace(
            nodes=[SimpleNamespace(id="one", server="example.com", country_code="")]
        ),
        _country_resolver=_RunningWorker(),
    )

    node_runtime_service.start_country_ip_resolution(controller)
