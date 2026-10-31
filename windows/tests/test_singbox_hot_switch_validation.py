from types import SimpleNamespace as NS

from xray_fluent.engines.singbox import operations
from xray_fluent.engines.singbox.clash_api import SelectorSwitchResult
from xray_fluent.engines.singbox.runtime_planner import SingboxRuntimePlan, singbox_node_tag


def _plan() -> SingboxRuntimePlan:
    good, bad, other = (singbox_node_tag(name) for name in ("good", "bad", "other"))
    return SingboxRuntimePlan(
        outcome="native_singbox",
        source_path=None,
        text_hash="",
        singbox_config={
            "outbounds": [
                {"type": "vless", "tag": good},
                {"type": "vless", "tag": bad},
                {"type": "vless", "tag": other},
                {"type": "selector", "tag": "proxy", "outbounds": [good, bad, other], "default": good},
            ]
        },
        has_proxy_outbound=True,
        used_selected_node=True,
        clash_api_secret="secret",
        xray_sidecar=None,
        clash_api_selector="proxy",
        clash_api_node_signatures=(("good", "g"), ("bad", "b"), ("other", "o")),
    )


def _controller(results):
    calls = []

    def validate_config(_path, config):
        calls.append(config)
        return results.pop(0)

    logs = []
    controller = NS(
        state=NS(settings=NS(singbox_path="sing-box.exe")),
        singbox=NS(validate_config=validate_config),
        _log=logs.append,
        _plan_runtime_singbox=lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("no fallback expected")),
    )
    return controller, calls, logs


def test_rejected_hot_switch_member_is_pruned_instead_of_failing() -> None:
    controller, calls, _logs = _controller([
        (False, "FATAL decode config: outbounds[1].transport: x_padding_bytes cannot be disabled"),
        (True, ""),
    ])
    plan = _plan()

    plan, valid, _output = operations.validate_runtime_plan(controller, plan, None, tun_mode=False)

    assert valid
    tags = [item["tag"] for item in plan.singbox_config["outbounds"]]
    assert singbox_node_tag("bad") not in tags
    assert plan.singbox_config["outbounds"][-1]["outbounds"] == [singbox_node_tag("good"), singbox_node_tag("other")]
    assert dict(plan.clash_api_node_signatures) == {"good": "g", "other": "o"}
    assert len(calls) == 2


def test_rejected_selected_node_falls_back_to_single_node_plan() -> None:
    controller, _calls, _logs = _controller([
        (False, "FATAL decode config: outbounds[0].transport: broken"),
        (True, ""),
    ])
    single = _plan()
    single.clash_api_selector = ""
    controller._plan_runtime_singbox = lambda node, *, tun_mode, hot_switch: single if hot_switch is False else None

    plan, valid, _output = operations.validate_runtime_plan(controller, _plan(), None, tun_mode=True)

    assert valid and plan is single


def test_selector_default_overrides_cached_choice(monkeypatch) -> None:
    switched = []

    class Client:
        def __init__(self, *, secret):
            assert secret == "secret"

        def switch_selector(self, selector, target):
            switched.append((selector, target))
            return SelectorSwitchResult(selector, singbox_node_tag("old"), target, changed=True)

    monkeypatch.setattr(operations, "SingboxClashApiClient", Client)
    logs = []

    assert operations.enforce_selector_default(NS(_log=logs.append), _plan())
    assert switched == [("proxy", singbox_node_tag("good"))]
    assert any("cached choice ignored" in line for line in logs)
