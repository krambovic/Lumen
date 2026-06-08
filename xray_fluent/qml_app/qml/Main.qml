import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import QtQuick.Window
import App 1.0
import "."

// Application shell: window + left navigation pane + page stack + toast host.
// Mirrors the original qfluentwidgets FluentWindow: the nav rail and title-bar
// area form ONE continuous Mica frame (the rail is transparent, no divider),
// and the active page sits in a solid rounded "window" panel inset into that
// frame — flush to the window's right/bottom edges with only the TOP-LEFT
// corner rounded, exactly like FluentWindow's stacked content widget.
//
// The saved theme (light / dark / system) and accent colour are resolved here
// and pushed into the Theme singleton + the Universal style, so every custom
// token and every built-in Control (Switch, ComboBox, ScrollBar, ...) tracks
// the same palette.
ApplicationWindow {
    id: win
    visible: true
    width: 1000
    height: 720
    minimumWidth: 600
    minimumHeight: 450
    title: App.appName
    // Transparent so the Windows 11 Mica backdrop (applied from main_qml.py via
    // DwmSetWindowAttribute) shows through the frame around the content panel.
    color: "transparent"

    // Background run: closing the window hides it to the system tray instead of
    // quitting, so the proxy/TUN keep running. The tray (built in main_qml.py)
    // owns the real exit through its «Выход» action, which sets App.quitting
    // before app.quit(); guard against that here. When no tray is available
    // (App.trayAvailable is false) we let the close through and the app quits.
    onClosing: (close) => {
        if (App.trayAvailable && !App.quitting) {
            close.accepted = false
            win.hide()
            App.notifyHiddenToTray()
        }
    }

    // Solid fallback base. When Mica IS active the DWM backdrop is composited
    // behind this; we keep it fully transparent so Mica is visible.
    Rectangle {
        anchors.fill: parent
        color: Theme.micaBase
        z: -1
    }

    // Segoe Fluent Icons ships with Windows 11 (Segoe MDL2 Assets on Win10);
    // these are the same glyphs qfluentwidgets' FluentIcon draws.
    readonly property string iconFont: "Segoe Fluent Icons"

    // Navigation rail expand/collapse. Collapsed (icon-only) by default, just
    // like the original FluentWindow; the hamburger toggles it. A saved
    // "compact" interface preference keeps it permanently collapsed.
    property bool navExpanded: false
    readonly property bool railCollapsed: !navExpanded

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
    }

    Universal.theme: Theme.dark ? Universal.Dark : Universal.Light
    Universal.accent: Theme.accent
    Universal.foreground: Theme.text
    // Opaque so built-in pop-ups (ComboBox dropdowns, Menu, Popup, ToolTip)
    // are NOT see-through. Cards/panes draw their own translucent Theme.card
    // fill, so this only affects floating control surfaces.
    Universal.background: Theme.flyout

    // Ordered page entries. Index == StackLayout child index.
    readonly property var nav: [
        { index: 0, label: "Панель",     glyph: "\uE80F", section: "top"    },
        { index: 1, label: "Серверы",    glyph: "\uEC05", section: "top"    },
        { index: 2, label: "Маршруты",   glyph: "\uE774", section: "top"    },
        { index: 3, label: "Конфиги",    glyph: "\uE943", section: "top",    hideOnCompact: true },
        { index: 4, label: "Zapret",     glyph: "\uE945", section: "top"    },
        { index: 5, label: "Логи",       glyph: "\uE8FD", section: "top",    hideOnCompact: true },
        { index: 6, label: "История",    glyph: "\uE81C", section: "top",    hideOnCompact: true },
        // Visual order matches the original FluentWindow bottom block:
        // О проекте → Обновления → Настройки. `index` stays bound to the
        // StackLayout child position; only the array order (= render order) changes.
        { index: 9, label: "О проекте",  glyph: "\uE946", section: "bottom" },
        { index: 7, label: "Обновления", glyph: "\uE777", section: "bottom" },
        { index: 8, label: "Настройки",  glyph: "\uE713", section: "bottom" }
    ]
    property int currentIndex: 0

    // When compact mode is enabled while an advanced page is open, fall back to
    // the dashboard so the StackLayout never shows a page whose nav item is hidden.
    Connections {
        target: App
        function onSettingsChanged() {
            if (App.compactMode && [3, 5, 6].indexOf(win.currentIndex) !== -1)
                win.currentIndex = 0
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // ── navigation pane ──────────────────────
        // Transparent (part of the Mica frame) with NO divider, so it merges
        // with the title-bar area; the content panel's own top-left corner +
        // hairline is what separates them, exactly like FluentWindow.
        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: win.railCollapsed ? Theme.railWidthCompact : Theme.railWidth
            color: "transparent"
            Behavior on Layout.preferredWidth { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }

            ColumnLayout {
                anchors.fill: parent
                anchors.topMargin: 8
                anchors.bottomMargin: 14
                spacing: 2

                // hamburger toggle (mirrors NavButton's internal layout so the
                // glyph lines up exactly with the nav icons below and tracks
                // the same collapse transition instead of animating its own x)
                Item {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 40
                    Rectangle {
                        anchors.fill: parent
                        anchors.leftMargin: 6
                        anchors.rightMargin: 6
                        radius: Theme.radiusSmall
                        color: burgerHover.hovered ? Theme.cardHover : "transparent"
                        Behavior on color { ColorAnimation { duration: 110 } }
                        RowLayout {
                            anchors.fill: parent
                            // Fixed offset = the icon's centred position in the
                            // collapsed rail, so it never snaps/jumps on toggle
                            // and lines up exactly with the NavButton icons.
                            anchors.leftMargin: 11
                            anchors.rightMargin: 12
                            spacing: 12
                            Text {
                                text: "\uE700"  // GlobalNavButton (hamburger)
                                font.family: win.iconFont
                                font.pixelSize: 16
                                color: Theme.textMuted
                                Layout.alignment: Qt.AlignVCenter
                                horizontalAlignment: Text.AlignLeft
                            }
                            Item { Layout.fillWidth: true; visible: !win.railCollapsed }
                        }
                        HoverHandler { id: burgerHover }
                        TapHandler { onTapped: win.navExpanded = !win.navExpanded }
                    }
                }

                // Small gap between the hamburger and the first nav item, like
                // the original FluentWindow (no brand/logo block in the rail).
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
                        onClicked: win.currentIndex = modelData.index
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
            Layout.fillWidth: true
            Layout.fillHeight: true
            // Paint above the (transparent) nav rail so rail buttons can never
            // visually bleed onto the content panel — fixes the long-standing
            // fullscreen overlap where nav items crept over the main window.
            z: 1

            Rectangle {
                id: contentWindow
                anchors.fill: parent
                anchors.rightMargin: -Theme.radius
                anchors.bottomMargin: -Theme.radius
                // Nudge the main window panel a couple of pixels down so the nav
                // rail buttons never overlap it when the window is maximised or
                // full-screen (no native title-bar gap there to absorb it).
                anchors.topMargin: (win.visibility === Window.FullScreen
                                    || win.visibility === Window.Maximized) ? 6 : 0
                clip: true
                radius: Theme.radius
                // Slightly lighter than the bare Mica frame so the panel reads as
                // a distinct "window"; still translucent so a hint of Mica shows.
                color: Theme.dark ? Qt.rgba(1, 1, 1, 0.045) : Qt.rgba(1, 1, 1, 0.45)
                // Only the top & left borders are visible (right/bottom are off
                // the window edge), defining the panel against the frame.
                border.width: 1
                border.color: Theme.divider

                StackLayout {
                    anchors.fill: parent
                    // Real padding from the visible edges; right/bottom add the
                    // overflow back so content keeps a 24/20 gutter on screen.
                    anchors.leftMargin: 24
                    anchors.topMargin: 20
                    anchors.rightMargin: 24 + Theme.radius
                    anchors.bottomMargin: 20 + Theme.radius
                    currentIndex: win.currentIndex

                    DashboardPage {}
                    NodesPage {}
                    RoutingPage {}
                    ConfigsPage {}
                    ZapretPage {}
                    LogsPage {}
                    HistoryPage {}
                    UpdatesPage {}
                    SettingsPage {}
                    AboutPage {}
                }
            }
        }
    }

    // toast overlay sits above everything
    ToastHost {}

    // ── lock overlay: blocks the whole UI until the master password is entered ──
    // Shown whenever the controller reports a locked state (auto-lock timeout or
    // the manual «Заблокировать» button). Mirrors the original app, which gates
    // access behind the master password instead of silently doing nothing.
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
                    text: "Приложение заблокировано"
                    color: Theme.text; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                }
                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: "Введите мастер-пароль для разблокировки"
                    color: Theme.textFaint; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap
                }
                TextField {
                    id: lockField
                    Layout.fillWidth: true
                    echoMode: TextInput.Password
                    placeholderText: "Пароль"
                    onAccepted: lockOverlay.tryUnlock()
                }
                Text {
                    id: lockError
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    visible: false
                    text: "Неверный пароль"
                    color: Theme.danger; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                }
                AccentButton {
                    Layout.fillWidth: true
                    kind: "accent"
                    glyph: "\uE785"
                    text: "Разблокировать"
                    onClicked: lockOverlay.tryUnlock()
                }
            }
        }
    }
}
