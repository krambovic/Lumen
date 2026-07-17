from pathlib import Path


QML_DIR = Path(__file__).parents[1] / "xray_fluent" / "qml_app" / "qml"


def test_shared_fluent_dialog_defines_common_surface_and_actions() -> None:
    source = (QML_DIR / "FluentDialog.qml").read_text(encoding="utf-8")

    assert "color: Theme.flyout" in source
    assert "border.color: Theme.flyoutBorder" in source
    assert "Overlay.modal: Rectangle" in source
    assert 'kind: "ghost"' in source
    assert 'kind: "accent"' in source


def test_pages_do_not_fall_back_to_native_dialog_buttons() -> None:
    qml_sources = {
        path.name: path.read_text(encoding="utf-8")
        for path in QML_DIR.glob("*.qml")
    }

    assert all("standardButtons:" not in source for source in qml_sources.values())
    assert "component FluentDialog:" not in qml_sources["RoutingPage.qml"]
    assert qml_sources["RoutingPage.qml"].count("FluentDialog {") >= 5
    assert "FluentDialog {\n        id: resetSettingsDialog" in qml_sources["SettingsPage.qml"]
    assert "FluentDialog {\n        id: qrDialog" in qml_sources["NodesPage.qml"]


def test_subscription_properties_do_not_refresh_and_use_fluent_copy_menu() -> None:
    source = (QML_DIR / "NodesPage.qml").read_text(encoding="utf-8")

    open_info = source[source.index("function openInfo()") : source.index("background: Rectangle", source.index("function openInfo()"))]
    assert "App.updateSubscription" not in open_info

    menu = source[source.index("id: subUrlMenu") : source.index("// ---- QR dialog")]
    assert "FluentMenu {\n        id: subUrlMenu" in source
    assert "FluentMenuItem" in menu

    shared_menu = (QML_DIR / "FluentMenu.qml").read_text(encoding="utf-8")
    shared_item = (QML_DIR / "FluentMenuItem.qml").read_text(encoding="utf-8")
    assert "color: Theme.flyout" in shared_menu
    assert "radius: 8" in shared_menu
    assert "border.color: Theme.flyoutBorder" in shared_menu
    assert "control.highlighted ? Theme.cardHover" in shared_item


def test_zero_subscription_limit_is_shown_as_unlimited() -> None:
    source = (QML_DIR / "NodesPage.qml").read_text(encoding="utf-8")

    assert "limitB <= 0" in source
    assert 'I18n.t("Безлимитный трафик")' in source


def test_text_controls_and_tooltips_use_shared_fluent_popups() -> None:
    context_menu = (QML_DIR / "TextEditContextMenu.qml").read_text(encoding="utf-8")
    tooltip = (QML_DIR / "FluentToolTip.qml").read_text(encoding="utf-8")
    accent_button = (QML_DIR / "AccentButton.qml").read_text(encoding="utf-8")

    assert "FluentMenu {" in context_menu
    assert 'text: I18n.t("Копировать")' in context_menu
    assert "color: Theme.flyout" in tooltip
    assert "radius: 8" in tooltip
    assert "y: parent ? -implicitHeight - 4 : 0" in tooltip
    assert "FluentToolTip {" in accent_button
    assert "cursorShape: Qt.IBeamCursor" in context_menu
    assert "Loader {" in context_menu


def test_requested_routing_and_config_editor_cleanup_is_kept() -> None:
    routing = (QML_DIR / "RoutingPage.qml").read_text(encoding="utf-8")
    configs = (QML_DIR / "ConfigsPage.qml").read_text(encoding="utf-8")
    nodes = (QML_DIR / "NodesPage.qml").read_text(encoding="utf-8")

    assert "route_exclude_address" not in routing
    assert "clientProfile" not in nodes
    assert 'I18n.t("Файл: ")' not in configs
    assert 'I18n.t("Шаблон: ")' not in configs
    assert 'I18n.t("Статус")' not in configs
    assert "App.openConfigDirectory(page.core)" in configs
    assert "coreTabMouse.containsMouse" in configs


def test_nodes_page_keeps_group_subscription_and_ping_feedback_controls() -> None:
    nodes = (QML_DIR / "NodesPage.qml").read_text(encoding="utf-8")

    assert 'I18n.t("Нет подписки")' in nodes
    assert "function onSubscriptionImported(groupName)" in nodes
    assert "App.deleteGroup(groupName)" in nodes
    assert "required property bool pinging" in nodes
    assert "running: nodeRow.pinging" in nodes
