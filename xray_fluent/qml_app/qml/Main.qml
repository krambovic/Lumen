import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import QtQuick.Window
import App 1.0
import "."

ApplicationWindow {
    id: win
    visible: true
    width: 1280
    height: 720
    minimumWidth: 640
    minimumHeight: 360
    title: App.appName
    color: "transparent"

    onClosing: (close) => {
        if (App.trayAvailable && !App.quitting) {
            close.accepted = false
            win.hide()
            App.notifyHiddenToTray()
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.micaBase
        z: -1
    }

    readonly property string iconFont: "Segoe Fluent Icons"
    property bool navExpanded: false
    readonly property bool railCollapsed: !navExpanded
    property real railVisualWidth: railCollapsed ? Theme.railWidthCompact : Theme.railWidth
    property bool railAnimating: false
    readonly property real targetContentWidth: Math.max(
        0,
        width - (navExpanded ? Theme.railWidth : Theme.railWidthCompact)
    )
    Behavior on railVisualWidth {
        NumberAnimation {
            duration: Theme.animations ? 155 : 0
            easing.type: Easing.OutCubic
            onRunningChanged: win.railAnimating = running
        }
    }

    // ---- Theme resolution ---------------------------------------------
    function resolveDark() {
        if (App.themeName === "dark")
            return true;
        if (App.themeName === "light")
            return false;
        return Application.styleHints.colorScheme === Qt.Dark; // "system"
    }

    Component.onCompleted: {
        Theme.accent = Qt.binding(function() { return App.accentColor; });
        Theme.dark = Qt.binding(function() { return win.resolveDark(); });
        Theme.density = Qt.binding(function() { return App.uiDensity; });
        Theme.cornerRadius = Qt.binding(function() { return App.uiCornerRadius; });
        Theme.animations = Qt.binding(function() { return App.uiAnimations; });
        Theme.backdrop = Qt.binding(function() { return App.uiBackdrop; });
    }

    Universal.theme: Theme.dark ? Universal.Dark : Universal.Light
    Universal.accent: Theme.accent
    Universal.foreground: Theme.text
    Universal.background: Theme.flyout

    // Ordered page entries. Index == StackLayout child index.
    readonly property var nav: [
        { index: 0, label: I18n.t("Панель"),     glyph: "\uE80F", section: "top"    },
        { index: 1, label: I18n.t("Серверы"),    glyph: "\uEC05", section: "top"    },
        { index: 2, label: I18n.t("Маршруты"),   glyph: "\uE774", section: "top"    },
        { index: 3, label: I18n.t("Конфиги"),    glyph: "\uE943", section: "top",    hideOnCompact: true },
        { index: 4, label: "Zapret",     glyph: "\uE945", section: "top"    },
        { index: 5, label: I18n.t("Логи"),       glyph: "\uE8FD", section: "top",    hideOnCompact: true },
        { index: 6, label: I18n.t("История"),    glyph: "\uE81C", section: "top",    hideOnCompact: true },
        { index: 9, label: I18n.t("О проекте"),  glyph: "\uE946", section: "bottom" },
        { index: 7, label: I18n.t("Обновления"), glyph: "\uE777", section: "bottom" },
        { index: 8, label: I18n.t("Настройки"),  glyph: "\uE713", section: "bottom" }
    ]
    property int currentIndex: 0

    Connections {
        target: App
        function onSettingsChanged() {
            if (App.compactMode && [3, 5, 6].indexOf(win.currentIndex) !== -1)
                win.currentIndex = 0
        }
    }

    Item {
        anchors.fill: parent

        // ── navigation pane ──────────────────────
        Rectangle {
            id: railClip
            width: win.railVisualWidth
            height: parent.height
            clip: true
            color: "transparent"
            z: 2

            ColumnLayout {
                width: Theme.railWidth
                height: parent.height
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.topMargin: 8
                anchors.bottomMargin: 14
                spacing: 2

                Item {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 40
                    Rectangle {
                        x: 6
                        y: 0
                        width: win.railCollapsed ? 38 : Math.max(38, parent.width - 12)
                        height: parent.height
                        radius: Theme.radiusSmall
                        color: burgerHover.hovered ? Theme.cardHover : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.animations ? 130 : 0; easing.type: Theme.easeStandard } }
                        Behavior on width { NumberAnimation { duration: Theme.animations ? 170 : 0; easing.type: Theme.easeEmphasized } }
                        Text {
                            text: "\uE700"  // GlobalNavButton
                            font.family: win.iconFont
                            font.pixelSize: 16
                            color: Theme.textMuted
                            anchors.left: parent.left
                            anchors.leftMargin: 11
                            anchors.verticalCenter: parent.verticalCenter
                            width: 18
                            horizontalAlignment: Text.AlignLeft
                            Behavior on color { ColorAnimation { duration: Theme.animations ? 140 : 0; easing.type: Theme.easeStandard } }
                        }
                        HoverHandler { id: burgerHover }
                        TapHandler { onTapped: win.navExpanded = !win.navExpanded }
                    }
                }

                Item { Layout.fillWidth: true; Layout.preferredHeight: 4 }

                // top section
                Repeater {
                    model: win.nav
                    delegate: NavButton {
                        Layout.fillWidth: true
                        visible: modelData.section === "top" && !(App.compactMode && modelData.hideOnCompact === true)
                        label: modelData.label
                        glyph: modelData.glyph
                        iconFont: win.iconFont
                        compact: win.railCollapsed
                        selected: win.currentIndex === modelData.index
                        onClicked: win.currentIndex = modelData.index
                    }
                }

                Item { Layout.fillHeight: true }

                // bottom section
                Repeater {
                    model: win.nav
                    delegate: NavButton {
                        Layout.fillWidth: true
                        visible: modelData.section === "bottom" && !(App.compactMode && modelData.hideOnCompact === true)
                        label: modelData.label
                        glyph: modelData.glyph
                        iconFont: win.iconFont
                        compact: win.railCollapsed
                        selected: win.currentIndex === modelData.index
                        badge: modelData.index === 7 && App.updatesAvailable
                        onClicked: { win.currentIndex = modelData.index; if (modelData.index === 7) App.markUpdatesSeen() }
                    }
                }
            }
        }

        // ── content ─────────────────────────
        // The active page lives in a solid rounded "window" panel inset into the
        // Mica frame. It is flush to the window's right & bottom edges with only
        // the top-left corner rounded — achieved by extending the panel's right
        // and bottom edges a little past the window (negative margins), so those
        // rounded corners fall outside the window and are clipped to straight
        // edges, leaving just the top-left curve visible (mirrors FluentWindow).
        Item {
            id: contentHost
            x: win.railVisualWidth
            y: 0
            width: Math.max(0, parent.width - win.railVisualWidth)
            height: parent.height
            clip: true
            z: 1

            Rectangle {
                id: contentWindow
                anchors.fill: parent
                anchors.rightMargin: -Theme.radius
                anchors.bottomMargin: -Theme.radius
                anchors.topMargin: (win.visibility === Window.FullScreen
                                    || win.visibility === Window.Maximized) ? 6 : 0
                clip: true
                radius: Theme.radius
                color: Theme.backdrop === "acrylic"
                       ? (Theme.dark ? Qt.rgba(0, 0, 0, 0.32) : Qt.rgba(1, 1, 1, 0.62))
                       : (Theme.dark ? Qt.rgba(1, 1, 1, 0.045) : Qt.rgba(1, 1, 1, 0.45))
                border.width: 1
                border.color: Theme.divider

                Item {
                    id: pageStack
                    anchors.left: parent.left
                    anchors.leftMargin: 24
                    anchors.top: parent.top
                    anchors.topMargin: 20
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 20 + Theme.radius
                    // During the 155 ms pane transition, heavy pages receive
                    // their final width once and are cached as one texture.
                    // The outer panel still follows the original x/width
                    // animation exactly, without relaying out every card each frame.
                    width: Math.max(
                        0,
                        (win.railAnimating ? win.targetContentWidth : contentHost.width) - 48
                    )
                    layer.enabled: win.railAnimating
                    layer.smooth: true
                    readonly property int slide: 6

                    DashboardPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 0 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 0 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                    NodesPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 1 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 1 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                    RoutingPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 2 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 2 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                    ConfigsPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 3 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 3 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                    ZapretPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 4 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 4 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                    LogsPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 5 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 5 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                    HistoryPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 6 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 6 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                    UpdatesPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 7 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 7 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                    SettingsPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 8 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 8 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                    AboutPage {
                        anchors.fill: parent; visible: opacity > 0
                        opacity: win.currentIndex === 9 ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 220 : 0; easing.type: Theme.easeStandard } }
                        transform: Translate { y: win.currentIndex === 9 ? 0 : pageStack.slide
                            Behavior on y { NumberAnimation { duration: Theme.animations ? 260 : 0; easing.type: Theme.easeEmphasized } } }
                    }
                }
            }
        }
    }

    // toast overlay sits above everything
    ToastHost {}

    // ── lock overlay: blocks the whole UI until the master password is entered ──
    Rectangle {
        id: lockOverlay
        anchors.fill: parent
        z: 9999
        visible: App.locked
        color: "#99000000"

        function tryUnlock() {
            if (App.unlock(lockField.text)) {
                lockField.text = ""
                lockError.visible = false
            } else {
                lockError.visible = true
                lockField.selectAll()
                lockField.forceActiveFocus()
            }
        }

        // Reset + focus the field every time the overlay appears.
        onVisibleChanged: if (visible) { lockError.visible = false; lockField.text = ""; lockField.forceActiveFocus() }

        // Swallow every click so the UI behind the overlay stays inaccessible.
        MouseArea { anchors.fill: parent; hoverEnabled: true; preventStealing: true }

        Rectangle {
            anchors.centerIn: parent
            width: 360
            radius: Theme.radius
            color: Theme.flyout
            border.width: 1
            border.color: Theme.flyoutBorder
            implicitHeight: lockCol.implicitHeight + 48

            ColumnLayout {
                id: lockCol
                anchors.centerIn: parent
                width: parent.width - 48
                spacing: 14

                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "\uEB95"
                    font.family: "Segoe Fluent Icons"; font.pixelSize: 40
                    color: Theme.accent
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: I18n.t("Приложение заблокировано")
                    color: Theme.text; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                }
                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: I18n.t("Введите мастер-пароль для разблокировки")
                    color: Theme.textFaint; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap
                }
                TextField {
                    id: lockField
                    Layout.fillWidth: true
                    echoMode: TextInput.Password
                    placeholderText: I18n.t("Пароль")
                    onAccepted: lockOverlay.tryUnlock()
                }
                Text {
                    id: lockError
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    visible: false
                    text: I18n.t("Неверный пароль")
                    color: Theme.danger; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                }
                AccentButton {
                    Layout.fillWidth: true
                    kind: "accent"
                    glyph: "\uE785"
                    text: I18n.t("Разблокировать")
                    onClicked: lockOverlay.tryUnlock()
                }
            }
        }
    }
}
