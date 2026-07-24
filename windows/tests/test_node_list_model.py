from __future__ import annotations

import sys

from PyQt6.QtCore import QCoreApplication

from xray_fluent.i18n import get_language, set_language
from xray_fluent.models import Node
from xray_fluent.qml_app.bridge.node_list_model import NodeListModel


def _app() -> QCoreApplication:
    return QCoreApplication.instance() or QCoreApplication(sys.argv)


def test_failed_ping_marks_node_as_tested_and_offline() -> None:
    _app()
    node = Node(name="dead", server="203.0.113.1", port=443)
    model = NodeListModel()
    model.set_nodes([node], selected_id=None)
    idx = model.index(0, 0)

    assert model.data(idx, NodeListModel.TestedRole) is False
    assert model.data(idx, NodeListModel.AliveRole) is False

    model.update_ping(node.id, None)

    assert model.data(idx, NodeListModel.TestedRole) is True
    assert model.data(idx, NodeListModel.AliveRole) is False
    assert model.data(idx, NodeListModel.PingRole) == -1


def test_failed_speed_can_override_previous_alive_status() -> None:
    _app()
    node = Node(name="slow", server="203.0.113.2", port=443, is_alive=True)
    model = NodeListModel()
    model.set_nodes([node], selected_id=None)
    idx = model.index(0, 0)

    model.update_speed(node.id, None)
    model.update_alive(node.id, False)

    assert model.data(idx, NodeListModel.TestedRole) is True
    assert model.data(idx, NodeListModel.AliveRole) is False
    assert model.data(idx, NodeListModel.SpeedRole) == -1.0


def test_pinging_role_is_visible_until_the_node_result_arrives() -> None:
    _app()
    first = Node(name="first", server="203.0.113.10", port=443)
    second = Node(name="second", server="203.0.113.11", port=443)
    model = NodeListModel()
    model.set_nodes([first, second], selected_id=None)

    model.set_pinging_ids([first.id, second.id])
    assert model.data(model.index(0, 0), NodeListModel.PingingRole) is True
    assert model.data(model.index(1, 0), NodeListModel.PingingRole) is True

    model.update_ping(first.id, 42)
    assert model.data(model.index(0, 0), NodeListModel.PingingRole) is False
    assert model.data(model.index(1, 0), NodeListModel.PingingRole) is True

    model.clear_pinging()
    assert model.data(model.index(1, 0), NodeListModel.PingingRole) is False


def test_auto_node_without_description_gets_default_description() -> None:
    _app()
    previous_language = get_language()
    set_language("ru")
    try:
        nodes = [
            Node(name="auto", scheme="auto"),
            Node(name="xray-auto", outbound={"protocol": "xray_config"}),
            Node(name="custom", scheme="auto", description="Провайдер выберет сервер"),
        ]
        model = NodeListModel()
        model.set_nodes(nodes, selected_id=None)

        assert model.data(model.index(0, 0), NodeListModel.DescriptionRole) == "Автовыбор лучших серверов"
        assert model.data(model.index(1, 0), NodeListModel.DescriptionRole) == "Автовыбор лучших серверов"
        assert model.data(model.index(2, 0), NodeListModel.DescriptionRole) == "Провайдер выберет сервер"
    finally:
        set_language(previous_language)
