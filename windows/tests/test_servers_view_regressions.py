from __future__ import annotations

import base64
from pathlib import Path
import subprocess
import sys

from PyQt6.QtCore import QCoreApplication

from xray_fluent.application import node_service
from xray_fluent.models import Node
from xray_fluent.qml_app.bridge.node_list_model import NodeListModel
from xray_fluent.subscription_presentation import EXTRA_PROVIDER_PARAMETERS, present_subscription


def test_mutated_node_invalidates_cached_country_miss_without_reimport():
    app = QCoreApplication.instance() or QCoreApplication([])
    model = NodeListModel()
    node = Node(name="Unknown", server="example.invalid")
    model.set_nodes([node], None)
    index = model.index(0, 0)
    assert model.data(index, model.CountryRole) == ""
    node.name = "Germany Frankfurt"
    model.set_nodes([node], None)
    assert model.data(index, model.CountryRole) == "de"
    node.name = "Japan Tokyo"
    assert model.data(index, model.CountryRole) == "jp"
    assert app is not None


def test_replacement_is_atomic_and_all_32_ids_exist_at_notification():
    app = QCoreApplication.instance() or QCoreApplication([])
    model = NodeListModel()
    observed = []
    model.modelReset.connect(lambda: observed.append([model.node_id_at(i) for i in range(model.rowCount())]))
    for generation, count in enumerate((10, 32, 32, 4, 32)):
        nodes = [Node(id=f"{generation}-{i}", name=f"Node {i}") for i in range(count)]
        model.set_nodes(nodes, nodes[-1].id)
        assert observed[-1] == [n.id for n in nodes]
        assert [model.index_of_id(n.id) for n in nodes] == list(range(count))
        assert model.data(model.index(count - 1, 0), model.SelectedRole) is True
    assert len(observed) == 5
    nodes[0].name = "Renamed in place"
    model.set_nodes(nodes, nodes[0].id)
    assert len(observed) == 5
    assert model.data(model.index(0, 0), model.NameRole) == "Renamed in place"
    assert app is not None


def test_all_new_documented_directives_are_retained_in_headers_and_body():
    for key in EXTRA_PROVIDER_PARAMETERS:
        headers = node_service._extract_subscription_metadata({key.upper(): "0"}, "test")
        body, info = node_service._extract_happ_body_metadata("#" + key + ": 0\nvless://example")
        assert headers["premiumFeatures"][key] == "0", key
        assert info["premiumFeatures"][key] == "0", key
        assert body == "vless://example"


def test_happ_expiry_priority_disable_and_safe_button():
    info = {"providerId": "test-provider", "expire": 1000 + 86400, "premiumFeatures": {
        "sub-expire": "1", "sub-expire-button-link": "javascript:alert(1)",
        "sub-info-text": "Maintenance", "sub-info-color": "green"}}
    result = present_subscription({"userinfo": info}, now=1000)
    assert [b["kind"] for b in result["banners"]] == ["happ-expiry"]
    assert result["banners"][0]["url"] == ""
    info["premiumFeatures"]["sub-expire"] = "0"
    result = present_subscription({"userinfo": info}, now=1000)
    assert result["banners"][0]["text"] == "Maintenance"
    info["premiumFeatures"]["sub-info-text"] = "0"
    assert present_subscription({"userinfo": info}, now=1000)["banners"] == []


def test_incy_banner_headers_override_panel_and_device_limit_does_not_drop_nodes():
    payload = {"isPremium": True, "settings": {"bannerEnabled": True, "bannerText": "Panel", "bannerButtonUrl": "https://panel.example"},
               "bannerText": "Header", "premiumUrl": "https://premium.example", "links": ["vless://example"]}
    import json
    raw = json.dumps(payload)
    body, info = node_service._extract_userinfo_from_body(raw)
    assert body == raw
    result = present_subscription({"userinfo": info})
    assert result["banners"][0]["text"] == "Header"
    assert result["banners"][0]["url"] == "https://premium.example"
    info["deviceLimitExceeded"] = True
    assert present_subscription({"userinfo": info})["banners"] == []
    assert present_subscription({"userinfo": info})["notice"]


def test_base64_banners_contacts_and_secret_fields():
    encoded = base64.b64encode("Обслуживание".encode()).decode()
    _, info = node_service._extract_happ_body_metadata("#banner-text: base64:" + encoded)
    assert info["bannerText"] == "Обслуживание"
    info.update({"supportEmail": "help@example.com", "supportUrl": "file:///private", "premiumFeatures": {"socks-auth-password": "private", "unknown-option": "value"}})
    result = present_subscription({"userinfo": info})
    assert result["links"] == [{"label": "Email", "url": "mailto:help@example.com"}]
    assert result["featureRows"][0]["value"] == "••••"
    assert result["featureRows"][1]["status"] == "Получено; не применяется автоматически"


def test_servers_page_replaces_scrolls_and_renders_flags_in_isolated_window():
    root = Path(__file__).parents[1]
    script = """
import os
os.environ['QT_QUICK_BACKEND'] = 'software'
os.environ['QML_DISABLE_DISK_CACHE'] = '1'
import time
from pathlib import Path
from PyQt6.QtCore import QUrl, QPointF, qInstallMessageHandler
from PyQt6.QtWidgets import QApplication
from PyQt6.QtGui import QFontDatabase
from PyQt6.QtQuick import QQuickWindow, QQuickItem
from PyQt6.QtQml import QQmlComponent, QQmlEngine, QQmlExpression, qmlRegisterSingletonInstance
import xray_fluent
from xray_fluent.models import Node
from xray_fluent.qml_app.bridge.app_bridge import AppBridge
app = QApplication([])
font = Path('C:/Windows/Fonts/segoeui.ttf')
if font.is_file():
    QFontDatabase.addApplicationFont(str(font))
bridge = AppBridge()
controller = bridge.controller
controller._load_state = "loaded"
controller.start_deferred_services = lambda: None
controller._schedule_country_resolution = lambda: None
controller._cleanup_tun_adapter = lambda **kwargs: None
controller.proxy.disable = lambda **kwargs: False
qmlRegisterSingletonInstance('App', 1, 0, 'App', bridge)
engine = QQmlEngine()
qml = Path(xray_fluent.__file__).parent / 'qml_app' / 'qml'
engine.addImportPath(str(qml))
messages = []
qInstallMessageHandler(lambda kind, context, message: messages.append(message))
window = QQuickWindow()
window.resize(1000, 720)
component = QQmlComponent(engine, QUrl.fromLocalFile(str(qml / 'NodesPage.qml')))

def pump():
    for _ in range(30):
        app.processEvents()
        time.sleep(0.005)

pump()
assert not component.isError(), [e.toString() for e in component.errors()]
page = component.create()
assert page is not None, [e.toString() for e in component.errors()]
page.setParentItem(window.contentItem())
page.setProperty('width', 1000)
page.setProperty('height', 720)
window.show()
pump()
view = page.findChild(QQuickItem, 'serverList')
assert view is not None
context = QQmlEngine.contextForObject(page)

def evaluate(code):
    expr = QQmlExpression(context, page, code)
    value, undefined = expr.evaluate()
    assert not expr.hasError(), expr.error().toString()
    return value

for generation, count in enumerate((10, 32, 32, 6, 32)):
    nodes = [Node(id=str(generation) + '-' + str(i), name='Node ' + str(i), server='example.invalid', port=443, country_code=('DE', 'FR', 'JP')[i % 3]) for i in range(count)]
    controller.state.nodes = nodes
    bridge._apply_node_model()
    pump()
    assert view.property('count') == count, (generation, count, view.property('count'), len(controller.state.nodes), bridge._node_model.rowCount(), bridge._filter_group, bridge._filter_text, controller.profile_loaded, messages[-15:])
    for target in (0, count - 1, count // 2):
        evaluate('list.positionViewAtIndex(' + str(target) + ', ListView.Center); list.forceLayout();')
        pump()
        assert evaluate('list.itemAtIndex(' + str(target) + ') !== null'), (generation, target, view.property('height'), view.property('contentY'), view.property('originY'), messages[-15:])
        assert evaluate('list.itemAtIndex(' + str(target) + ').nodeId') == nodes[target].id
        assert evaluate('list.itemAtIndex(' + str(target) + ').height') > 0

evaluate('page.selectOnly(1); page.toggle(2);')
assert page.property('selCount') == 2
selected = evaluate('JSON.stringify(page.selectedIds().sort())')
bridge.setNodeSort('name', True)
pump()
assert page.property('selCount') == 2 and evaluate('JSON.stringify(page.selectedIds().sort())') == selected
bridge.setNodeFilter('', 'Node 31')
pump()
assert view.property('count') == 1 and evaluate('list.itemAtIndex(0).nodeId') == nodes[31].id
bridge.setNodeFilter('', '')
bridge.setNodeSort('manual', True)
pump()
assert view.property('count') == 32

controller.state.subscriptions = [{'id': 'demo' , 'name': 'Demo subscription', 'node_count': 32, 'url': 'https://example.invalid/sub', 'userinfo': {'providerId': 'demo', 'profileDescription': 'Provider information', 'premiumFeatures': {'sub-info-text': 'Maintenance notice', 'sub-info-color': 'blue', 'sub-info-button-text': 'Details', 'sub-info-button-link': 'https://example.invalid/info'}}}]
bridge._on_subscriptions_changed()
bridge.setSelectedSubscriptionId('demo')
pump()
assert evaluate('page.activeSub.id') == 'demo'
assert evaluate('page.providerView.banners.length') == 1
assert evaluate('providerBanners.count') == 1
assert evaluate('providerBanners.itemAt(0) !== null && providerBanners.itemAt(0).visible')
controller.state.subscriptions.append({'id': 'pinned', 'name': 'Pinned provider', 'url': 'https://example.invalid/other', 'userinfo': {'providerId': 'pin', 'premiumFeatures': {'subscription-pin': 'true'}}})
bridge._on_subscriptions_changed()
pump()
assert evaluate('App.subscriptions[0].id') == 'pinned'
assert evaluate('page.activeSub.id') == 'demo'
assert evaluate('App.subscriptions[subCombo.currentIndex - 1].id') == 'demo'
evaluate('infoDialog.openInfo();')
pump()
assert evaluate('page.infoSub().id') == 'demo'
assert evaluate('page.infoRows(page.infoSub()).length') >= 5
evaluate('infoDialog.close();')
pump()
for width in (720, 1000):
    window.resize(width, 720)
    page.setProperty('width', width)
    pump()
    assert view.property('height') > 100
    evaluate('list.positionViewAtIndex(0, ListView.Beginning); list.forceLayout();')
    pump()
    image = evaluate('list.itemAtIndex(0)').findChild(QQuickItem, 'serverFlagImage')
    assert image is not None and image.isVisible()
    status = QQmlExpression(QQmlEngine.contextForObject(image), image, 'status === Image.Ready')
    assert status.evaluate()[0] is True and not status.hasError()
    frame = window.grabWindow()
    assert not frame.isNull()
    point = image.mapToScene(QPointF(image.width() / 2, image.height() / 2))
    pixel = frame.pixelColor(round(point.x()), round(point.y()))
    assert pixel.red() > 120 and pixel.green() < 100 and pixel.blue() < 100, pixel.name()
    if width == 1000:
        frame.save(str(qml.parents[3] / '.agent' / 'tmp' / 'servers-tab-preview.png'))
# A failed asynchronous decode must leave a painted local fallback, and
# a later valid source must recover without recycling the row.
source_url = image.property('source')
image.setProperty('source', QUrl('data:image/svg+xml;base64,PGJyb2tlbi8+'))
pump()
failed = QQmlExpression(QQmlEngine.contextForObject(image), image, 'status === Image.Error')
assert failed.evaluate()[0] is True
fallback = window.grabWindow().pixelColor(round(point.x()), round(point.y()))
assert fallback.red() > 120 and fallback.green() < 100 and fallback.blue() < 100, fallback.name()
image.setProperty('source', source_url)
pump()
assert status.evaluate()[0] is True and image.isVisible()
errors = [m for m in messages if 'NodesPage.qml' in m and any(k in m for k in ['ReferenceError', 'TypeError', 'Unable to assign', 'Binding loop', 'Required property'])]
assert not errors, errors
print('SERVERS_RENDER_OK: 10/32/reimport/filter replacement; scroll; SVG pixels; 720/1000px')
bridge.shutdown()
controller.finalize_shutdown()
window.close()
page.deleteLater()
engine.deleteLater()
app.processEvents()
"""
    result = subprocess.run([sys.executable, str(root / "scripts/run_safe_tests.py"), "--exec-fixture", base64.b64encode(script.encode()).decode()],
                            cwd=root, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30)
    assert result.returncode == 0, result.stdout + result.stderr
    assert "SERVERS_RENDER_OK" in result.stdout


def test_empty_headers_disable_old_banners_and_body_userinfo_is_retained():
    header = node_service._extract_subscription_metadata({"Banner-Text": "", "Banner-Button-Url": "", "Sub-Info-Text": "", "Hide-Url": ""}, "test")
    assert header["bannerText"] == ""
    assert header["bannerButtonUrl"] == ""
    assert header["premiumFeatures"]["sub-info-text"] == ""
    assert header["hideUrl"] is False
    info = {"isPremium": True, "providerSettings": {"bannerEnabled": True, "bannerText": "Old"}, **header}
    assert present_subscription({"userinfo": info})["banners"] == []
    _, body = node_service._extract_happ_body_metadata("#subscription-userinfo: upload=100; download=200; total=300; expire=1700000000")
    assert body["upload"] == 100 and body["download"] == 200 and body["total"] == 300
    assert body["expire"] == 1700000000


def test_subscription_refresh_keeps_usage_fields_omitted_by_fallback_response():
    stored = {"upload": 1024, "download": 2048, "total": 8192, "expire": 1700000000}
    fresh = {"clientProfile": "Happ", "networkPath": "direct"}

    merged = node_service._merge_stored_subscription_info(stored, fresh)

    assert merged["upload"] == 1024
    assert merged["download"] == 2048
    assert merged["total"] == 8192
    assert merged["clientProfile"] == "Happ"
    assert merged["networkPath"] == "direct"
    assert node_service._merge_stored_subscription_info(stored, {"upload": 0})["upload"] == 0


def test_premium_is_not_inferred_from_native_runtime_settings():
    import json
    payload = {"settings": {"bannerEnabled": True}, "theme": {"enabled": True}, "outbounds": [{"type": "direct"}]}
    raw = json.dumps(payload)
    body, info = node_service._extract_userinfo_from_body(raw)
    assert body == raw
    assert "providerSettings" not in info and "providerTheme" not in info
    assert present_subscription({"userinfo": info})["banners"] == []


def test_nested_premium_values_are_bounded_and_credentials_are_masked():
    info = {"providerSettings": {"nested": {"password": "secret-never-show", "token": "private-token", "large": "a" * 1000000}},
            "premiumFeatures": {"subscription-always-hwid-enable": "0"}}
    result = present_subscription({"userinfo": info})
    assert result["featureRows"][0]["value"] == "0"
    values = str(result["featureRows"])
    assert "secret-never-show" not in values and "private-token" not in values
    assert all(len(item["value"]) <= 512 for item in result["featureRows"])


def test_happ_banner_requires_provider_id_and_expiry_is_bounded():
    now = 1700000000
    info = {"expire": now + 3 * 86400 + 1, "premiumFeatures": {"sub-info-text": "Info", "sub-expire": "true", "sub-info-button-link": "https://user:password@example.com"}}
    assert present_subscription({"userinfo": info}, now=now)["banners"] == []
    info["providerId"] = "example"
    assert present_subscription({"userinfo": info}, now=now)["banners"][0]["kind"] == "happ-info"
    info["expire"] -= 1
    assert present_subscription({"userinfo": info}, now=now)["banners"][0]["kind"] == "happ-expiry"
    info["expire"] = (now - 1) * 1000
    assert present_subscription({"userinfo": info}, now=now)["banners"][0]["text"] == "Подписка истекла"
    del info["expire"]
    assert present_subscription({"userinfo": info}, now=now)["banners"][0]["kind"] == "happ-info"
    assert present_subscription({"userinfo": info}, now=now)["banners"][0]["url"] == ""


def test_incy_maintenance_has_priority_over_expiry_and_theme_stays_local():
    info = {"isPremium": True, "expire": 999, "providerSettings": {
        "bannerEnabled": True, "bannerText": "Maintenance", "expiryBannerEnabled": True, "expiryBannerText": "Expired"},
        "providerTheme": {"enabled": True, "accent": "#123456", "background": "file:///private"}}
    result = present_subscription({"userinfo": info}, now=1000)
    assert [b["kind"] for b in result["banners"]] == ["incy-info"]
    info["bannerText"] = ""
    result = present_subscription({"userinfo": info}, now=1000)
    assert [b["kind"] for b in result["banners"]] == ["incy-expiry"]
    assert all("не применяется" in row["status"] for row in result["featureRows"] if row["key"].startswith("INCY.theme."))
