from __future__ import annotations

import sys

from PyQt6.QtCore import QCoreApplication

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
