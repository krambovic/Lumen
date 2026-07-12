from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.application import auto_switch_service, node_service
from xray_fluent.models import Node


class _Signal:
    def emit(self, *args, **kwargs) -> None:
        pass


class _Controller:
    def __init__(self, nodes: list[Node], selected_id: str | None) -> None:
        self.state = SimpleNamespace(
            nodes=nodes,
            selected_node_id=selected_id,
            settings=SimpleNamespace(auto_connect_on_import=False),
        )
        self.nodes_changed = _Signal()
        self.selection_changed = _Signal()
        self.connected = False
        self._desired_connected = False
        self.transition_reasons: list[str] = []

    @property
    def selected_node(self) -> Node | None:
        return next((node for node in self.state.nodes if node.id == self.state.selected_node_id), None)

    def save(self) -> None:
        pass

    def schedule_save(self) -> None:
        pass

    def _start_country_ip_resolution(self) -> None:
        pass

    def _request_transition(self, reason: str) -> None:
        self.transition_reasons.append(reason)

    def _reset_auto_switch_state(self, **_kwargs) -> None:
        pass


def _node(node_id: str, group: str, *, speed: float | None = None, alive: bool | None = None) -> Node:
    return Node(
        id=node_id,
        name=node_id,
        scheme="vless",
        server=f"{node_id}.example.com",
        port=443,
        link=f"vless://{node_id}",
        outbound={"type": "vless"},
        group=group,
        speed_mbps=speed,
        is_alive=alive,
    )


def _patch_imported_nodes(nodes: list[Node]):
    original = (
        node_service.parse_links_text,
        node_service.normalize_node_outbound,
        node_service.validate_node_outbound,
        node_service.detect_country,
    )
    node_service.parse_links_text = lambda _text: (nodes, [])
    node_service.normalize_node_outbound = lambda _node: None
    node_service.validate_node_outbound = lambda _node: ""
    node_service.detect_country = lambda *_args: ""
    return original


def _restore_import_patches(original) -> None:
    (
        node_service.parse_links_text,
        node_service.normalize_node_outbound,
        node_service.validate_node_outbound,
        node_service.detect_country,
    ) = original


def test_direct_import_can_target_selected_group() -> None:
    fresh = _node("fresh", "Default")
    controller = _Controller([], None)

    original = _patch_imported_nodes([fresh])
    try:
        added, errors = node_service.import_nodes_from_text(
            controller,
            "fresh payload",
            group="Manual",
        )
    finally:
        _restore_import_patches(original)

    assert added == 1
    assert errors == []
    assert controller.state.nodes[0].group == "Manual"
    assert controller.state.selected_node_id is None


def test_direct_import_preserves_the_active_server_until_user_selects_new_one() -> None:
    active = _node("active", "Default")
    fresh = _node("fresh", "Default")
    controller = _Controller([active], "active")
    controller.connected = True
    controller._desired_connected = True

    original = _patch_imported_nodes([fresh])
    try:
        added, errors = node_service.import_nodes_from_text(controller, "fresh payload")
    finally:
        _restore_import_patches(original)

    assert added == 1
    assert errors == []
    assert controller.state.selected_node_id == "active"
    assert controller.transition_reasons == []

    node_service.set_selected_node(controller, "fresh")

    assert controller.state.selected_node_id == "fresh"
    assert controller.transition_reasons == ["node switched"]


def test_subscription_update_preserves_selection_from_other_group() -> None:
    active = _node("active", "Main")
    old_sub = _node("old-sub", "Sub")
    fresh_sub = _node("fresh-sub", "Sub")
    controller = _Controller([active, old_sub], "active")

    original = _patch_imported_nodes([fresh_sub])
    try:
        added, errors, _userinfo = node_service._apply_subscription_payload(
            controller,
            "https://example.com/sub",
            "Sub",
            ("fresh payload", {}, []),
            replace_existing_group=True,
        )
    finally:
        _restore_import_patches(original)

    assert added == 1
    assert errors == []
    assert controller.state.selected_node_id == "active"
    assert [node.id for node in controller.state.nodes] == ["active", "fresh-sub"]


def test_subscription_update_reselects_when_active_group_is_replaced() -> None:
    old_sub = _node("old-sub", "Sub")
    fresh_sub = _node("fresh-sub", "Sub")
    controller = _Controller([old_sub], "old-sub")
    controller.connected = True
    controller._desired_connected = True

    original = _patch_imported_nodes([fresh_sub])
    try:
        added, _errors, _userinfo = node_service._apply_subscription_payload(
            controller,
            "https://example.com/sub",
            "Sub",
            ("fresh payload", {}, []),
            replace_existing_group=True,
        )
    finally:
        _restore_import_patches(original)

    assert added == 1
    assert controller.state.selected_node_id == "fresh-sub"
    assert controller.transition_reasons == ["active subscription updated"]


def test_auto_switch_stays_inside_current_group() -> None:
    current = _node("current", "Main")
    same_group = _node("same", "Main", speed=10.0, alive=True)
    other_group = _node("other", "Other", speed=100.0, alive=True)
    controller = _Controller([current, other_group, same_group], "current")

    assert auto_switch_service.get_next_node_for_auto_switch(controller) is same_group


def test_auto_switch_does_not_jump_when_group_has_no_alternative() -> None:
    current = _node("current", "Main")
    other_group = _node("other", "Other", speed=100.0, alive=True)
    controller = _Controller([current, other_group], "current")

    assert auto_switch_service.get_next_node_for_auto_switch(controller) is None
