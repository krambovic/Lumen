from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.application import auto_switch_service, node_service
from xray_fluent.models import Node
from xray_fluent.qml_app.bridge import app_bridge


class _Signal:
    def emit(self, *args, **kwargs) -> None:
        pass


class _Controller:
    def __init__(self, nodes: list[Node], selected_id: str | None) -> None:
        self.state = SimpleNamespace(
            nodes=nodes,
            selected_node_id=selected_id,
            settings=SimpleNamespace(auto_connect_on_import=False),
            subscriptions=[],
            manual_groups=[],
        )
        self.nodes_changed = _Signal()
        self.selection_changed = _Signal()
        self.subscriptions_changed = _Signal()
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


def test_subscription_profile_title_becomes_group_only_when_name_is_empty() -> None:
    controller = _Controller([], None)
    link = (
        "vless://00000000-0000-0000-0000-000000000001@one.example:443"
        "?encryption=none&type=tcp&security=none#one"
    )

    added, errors = node_service.apply_fetched_subscription(
        controller,
        "https://sub.example/config",
        "",
        "import",
        link,
        {"profileTitle": "Provider title"},
        [],
    )

    assert added == 1
    assert errors == []
    assert controller.state.nodes[0].group == "Provider title"
    assert controller.state.subscriptions[0]["name"] == "Provider title"


def test_add_manual_node_persists_validated_node() -> None:
    controller = _Controller([], None)
    node = Node(
        name="Manual VLESS",
        scheme="vless",
        server="manual.example",
        port=443,
        group="Manual",
        outbound={
            "protocol": "vless",
            "settings": {
                "vnext": [{
                    "address": "manual.example",
                    "port": 443,
                    "users": [{"id": "00000000-0000-0000-0000-000000000001"}],
                }],
            },
            "streamSettings": {"network": "tcp", "security": "none"},
        },
    )

    node_id = node_service.add_manual_node(controller, node)

    assert node_id == node.id
    assert controller.state.nodes == [node]
    assert node.sort_order == 1


class _RecordingSignal:
    def __init__(self, events: list[tuple]) -> None:
        self.events = events

    def emit(self, *args) -> None:
        self.events.append(tuple(args))


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
    node_service.parse_links_text = lambda _text, **_kwargs: (nodes, [])
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


def test_same_server_can_be_imported_into_another_group_but_not_twice_into_one_group() -> None:
    existing = _node("existing", "Group B")
    candidate = _node("candidate", "Default")
    candidate.link = existing.link
    controller = _Controller([existing], None)

    original = _patch_imported_nodes([candidate])
    try:
        added, errors = node_service.import_nodes_from_text(controller, "same payload", group="Group A")
    finally:
        _restore_import_patches(original)

    assert added == 1
    assert errors == []
    assert [(node.link, node.group) for node in controller.state.nodes] == [
        (existing.link, "Group B"),
        (existing.link, "Group A"),
    ]

    duplicate = _node("duplicate", "Default")
    duplicate.link = existing.link
    original = _patch_imported_nodes([duplicate])
    try:
        added, errors = node_service.import_nodes_from_text(controller, "same payload", group="Group A")
    finally:
        _restore_import_patches(original)

    assert added == 0
    assert errors == []
    assert len(controller.state.nodes) == 2


def test_default_group_always_exists_without_nodes_or_manual_groups() -> None:
    controller = _Controller([], None)

    assert node_service.get_all_groups(controller) == ["Default"]


def test_subscription_server_is_not_blocked_by_same_link_in_another_group() -> None:
    existing = _node("existing", "Manual")
    candidate = _node("candidate", "Default")
    candidate.link = existing.link
    controller = _Controller([existing], None)

    original = _patch_imported_nodes([candidate])
    try:
        added, errors, _info = node_service._apply_subscription_payload(
            controller,
            "https://example.com/sub",
            "Subscription",
            ("same payload", {}, []),
        )
    finally:
        _restore_import_patches(original)

    assert added == 1
    assert errors == []
    assert {(node.link, node.group) for node in controller.state.nodes} == {
        (existing.link, "Manual"),
        (existing.link, "Subscription"),
    }


def test_delete_group_removes_its_nodes_subscription_and_manual_entry() -> None:
    grouped = _node("grouped", "Temporary")
    grouped.subscription_id = "sub-id"
    keep = _node("keep", "Default")
    controller = _Controller([grouped, keep], None)
    controller.state.manual_groups = ["Temporary", "Other"]
    controller.state.subscriptions = [
        {"id": "sub-id", "url": "https://example.com/sub", "name": "Temporary", "group": "Temporary"}
    ]

    assert node_service.delete_group(controller, "Temporary") is True
    assert controller.state.nodes == [keep]
    assert controller.state.subscriptions == []
    assert controller.state.manual_groups == ["Other"]
    assert node_service.delete_group(controller, "Default") is False


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


def test_subscription_update_retains_active_server_when_provider_omits_it() -> None:
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

    assert added == 2
    assert controller.state.selected_node_id == "old-sub"
    assert [node.id for node in controller.state.nodes] == ["fresh-sub", "old-sub"]
    assert controller.transition_reasons == []


def test_subscription_update_keeps_selected_server_when_credentials_change() -> None:
    controller = _Controller([], None)
    original = "\n".join((
        "vless://00000000-0000-0000-0000-000000000001@a.example:443?encryption=none&type=tcp#A",
        "vless://00000000-0000-0000-0000-000000000002@b.example:443?encryption=none&type=tcp#B",
    ))
    added, errors = node_service.apply_fetched_subscription(
        controller, "https://example.com/sub", "Sub", "import", original, {}, [],
    )
    assert (added, errors) == (2, [])
    selected = next(node for node in controller.state.nodes if node.name == "B")
    selected.ping_ms = 25
    selected.ping_kind = "proxy"
    selected.is_alive = True
    selected.ping_history = [25]
    controller.state.selected_node_id = selected.id

    updated = original.replace(
        "00000000-0000-0000-0000-000000000002@b.example",
        "00000000-0000-0000-0000-000000000099@b.example",
    )
    added, errors = node_service.apply_fetched_subscription(
        controller, "https://example.com/sub", "Sub", "update", updated, {}, [],
    )

    assert (added, errors) == (2, [])
    replacement = next(node for node in controller.state.nodes if node.name == "B")
    assert replacement.id == selected.id
    assert controller.state.selected_node_id == selected.id
    # A materially changed credential invalidates old health measurements.
    assert replacement.ping_ms is None
    assert replacement.is_alive is None
    assert replacement.ping_history == []


def test_failed_fetch_never_marks_cached_nodes_as_new_parser_revision(monkeypatch) -> None:
    subscription = {"parser_revision": 1, "etag": "old"}
    monkeypatch.setattr("random.uniform", lambda _low, _high: 1.0)

    node_service._mark_subscription_fetch_metadata(
        subscription,
        headers={"etag": "untrusted-new", "retry-after": "60"},
        status=503,
        error="temporary failure",
    )

    assert subscription["parser_revision"] == 1
    assert subscription["etag"] == "old"
    node_service._mark_subscription_fetch_metadata(
        subscription,
        headers={"etag": "fresh"},
        status=200,
        success=True,
    )
    assert subscription["parser_revision"] == node_service.SUBSCRIPTION_PARSER_REVISION
    assert subscription["etag"] == "fresh"


def test_subscription_hwid_never_leaks_the_raw_machine_guid(monkeypatch) -> None:
    machine_guid = "72b21638-4934-4a0a-ad11-bf13a9612c0e"
    node_service._windows_machine_hwid.cache_clear()
    monkeypatch.setattr(node_service, "_windows_machine_guid", lambda: machine_guid)
    monkeypatch.setattr(node_service, "get_install_id", lambda: "install-a")

    resolved = node_service._resolve_subscription_hwid("manual", use_real_hwid=True)

    assert resolved != machine_guid
    assert len(resolved) == 36
    node_service._windows_machine_hwid.cache_clear()
    assert node_service._resolve_subscription_hwid("manual", use_real_hwid=True) == resolved

    node_service._windows_machine_hwid.cache_clear()
    monkeypatch.setattr(node_service, "get_install_id", lambda: "install-b")
    assert node_service._resolve_subscription_hwid("manual", use_real_hwid=True) != resolved
    node_service._windows_machine_hwid.cache_clear()


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


def test_manual_server_selection_toast_is_emitted_before_reconnect(monkeypatch) -> None:
    events: list[tuple] = []
    selected = _node("new", "Main")
    selected.name = "New server"
    monkeypatch.setattr(app_bridge, "tr", lambda key, **_params: key)

    bridge = SimpleNamespace(
        _selected_id="old",
        _selected_name="Old server",
        _selected_flag="",
        _selected_flag_source="",
        _selected_latency=-1,
        _manual_selection_in_progress=False,
        _connected=False,
        toast=_RecordingSignal(events),
        trayNotify=_RecordingSignal(events),
        selectionChanged=_RecordingSignal(events),
        _node_model=SimpleNamespace(set_selected=lambda node_id: events.append(("model", node_id))),
    )

    class _SelectionController:
        def set_selected_node(self, node_id: str) -> None:
            assert node_id == selected.id
            app_bridge.AppBridge._on_selection_changed(bridge, selected)
            events.append(("reconnect",))

    bridge.controller = _SelectionController()

    app_bridge.AppBridge.selectNode(bridge, selected.id)

    assert ("info", "Сервер изменён: New server") in events
    assert events.index(("info", "Сервер изменён: New server")) < events.index(("reconnect",))
    assert bridge._manual_selection_in_progress is False


def test_restored_server_selection_does_not_show_manual_toast(monkeypatch) -> None:
    events: list[tuple] = []
    selected = _node("restored", "Main")
    monkeypatch.setattr(app_bridge, "tr", lambda key, **_params: key)
    bridge = SimpleNamespace(
        _selected_id="",
        _selected_name="",
        _selected_flag="",
        _selected_flag_source="",
        _selected_latency=-1,
        _manual_selection_in_progress=False,
        _connected=False,
        toast=_RecordingSignal(events),
        trayNotify=_RecordingSignal(events),
        selectionChanged=_RecordingSignal(events),
        _node_model=SimpleNamespace(set_selected=lambda _node_id: None),
    )

    app_bridge.AppBridge._on_selection_changed(bridge, selected)

    assert not any(event and event[0] == "info" for event in events)


def test_release_1912_subscription_preserves_958_credentials_on_250_endpoints(monkeypatch):
    from uuid import UUID
    monkeypatch.setattr(node_service, "detect_country", lambda *_a: "")
    controller = _Controller([], None)
    links = [f"vless://{UUID(int=i + 1)}@server-{i % 250}.example:443?encryption=none&type=tcp#node" for i in range(958)]
    added, errors = node_service.apply_fetched_subscription(controller, "https://example.test/sub", "Sub", "import", "\n".join(links), {}, [])
    assert (added, errors) == (958, [])
    original = {node.link: node.id for node in controller.state.nodes}
    for order in (list(reversed(links)), links):
        added, errors = node_service.apply_fetched_subscription(controller, "https://example.test/sub", "Sub", "update", "\n".join(order), {}, [])
        assert (added, errors) == (958, [])
        assert len({node.id for node in controller.state.nodes}) == 958
        assert {node.link: node.id for node in controller.state.nodes} == original


def test_release_1912_duplicate_provider_entries_do_not_accumulate_or_disappear():
    controller = _Controller([], None)
    uri = "vless://00000000-0000-0000-0000-000000000001@one.example:443?encryption=none&type=tcp#one"
    body = uri + "\n" + uri
    added, errors = node_service.apply_fetched_subscription(controller, "https://example.test/sub", "Sub", "import", body, {}, [])
    assert (added, errors) == (2, [])
    ids = {node.id for node in controller.state.nodes}
    assert len(ids) == 2
    added, errors = node_service.apply_fetched_subscription(controller, "https://example.test/sub", "Sub", "update", body, {}, [])
    assert (added, errors) == (2, [])
    assert {node.id for node in controller.state.nodes} == ids


def test_release_1912_rekey_cannot_steal_the_id_of_a_later_exact_match():
    controller = _Controller([], None)
    original = "vless://00000000-0000-0000-0000-000000000001@one.example:443?encryption=none&type=tcp#shared"
    replacement = original.replace("000000000001@", "000000000002@")
    assert node_service.apply_fetched_subscription(controller, "https://example.test/sub", "Sub", "import", original, {}, []) == (1, [])
    selected = controller.state.nodes[0]
    controller.state.selected_node_id = selected.id
    added, errors = node_service.apply_fetched_subscription(controller, "https://example.test/sub", "Sub", "update", replacement + "\n" + original, {}, [])
    assert (added, errors) == (2, [])
    assert controller.selected_node.id == selected.id
    assert controller.selected_node.link == original


def test_release_1912_partial_parse_cannot_replace_existing_subscription():
    controller = _Controller([], None)
    uri = "vless://00000000-0000-0000-0000-000000000001@one.example:443?encryption=none&type=tcp#one"
    assert node_service.apply_fetched_subscription(controller, "https://example.test/sub", "Sub", "import", uri, {}, []) == (1, [])
    before = list(controller.state.nodes)
    added, errors = node_service.apply_fetched_subscription(controller, "https://example.test/sub", "Sub", "update", uri + "\nnot-a-valid-node", {}, [])
    assert added == 0
    assert errors
    assert controller.state.nodes == before
