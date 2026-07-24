from xray_fluent.models import Node
from xray_fluent.node_transport import node_transport
from xray_fluent.qml_app.bridge.node_list_model import NodeListModel


def _node(protocol: str, outbound: dict | None = None) -> Node:
    return Node(scheme=protocol, outbound=outbound or {"protocol": protocol})


def test_xray_stream_transport_is_displayed() -> None:
    assert node_transport(_node("vless", {
        "protocol": "vless",
        "streamSettings": {"network": "xhttp"},
    })) == "XHTTP"
    assert node_transport(_node("vmess", {
        "protocol": "vmess",
        "streamSettings": {"network": "ws"},
    })) == "WS"
    assert node_transport(_node("trojan", {
        "protocol": "trojan",
        "streamSettings": {"network": "raw"},
    })) == "TCP"


def test_native_singbox_transport_is_displayed() -> None:
    assert node_transport(_node("vless", {
        "protocol": "vless",
        "singbox": {"transport": {"type": "grpc"}},
    })) == "GRPC"
    assert node_transport(_node("mieru", {
        "protocol": "mieru",
        "singbox": {"type": "mieru", "transport": "TCP"},
    })) == "MIERU/TCP"
    assert node_transport(_node("masque", {
        "protocol": "masque",
        "singbox": {"type": "masque"},
    })) == "MASQUE"


def test_transport_falls_back_to_protocol_carrier() -> None:
    assert node_transport(_node("vless")) == "TCP"
    assert node_transport(_node("wireguard")) == "UDP"
    assert node_transport(_node("unknown")) == "—"


def test_transport_is_exposed_by_node_list_model() -> None:
    model = NodeListModel()
    model.set_nodes([_node("vmess", {
        "protocol": "vmess",
        "streamSettings": {"network": "ws"},
    })], None)
    assert model.data(model.index(0, 0), model.TransportRole) == "WS"
