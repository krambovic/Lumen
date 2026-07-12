import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import QtQuick.Window
import QtQuick.Effects
import App 1.0
import "."

ApplicationWindow {
    id: win
    visible: false
    width: App.windowWidth > 0 ? App.windowWidth : 1280
    height: App.windowHeight > 0 ? App.windowHeight : 720
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
        color: Theme.windowBase
        z: -1
    }
    Image {
        id: wallpaperImg
        anchors.fill: parent
        z: -1
        source: App.uiWallpaper !== ""
                ? "file:///" + ("" + App.uiWallpaper).replace(/\\/g, "/")
                : ""
        visible: source != "" && status === Image.Ready
        fillMode: Image.PreserveAspectCrop
        opacity: App.uiWallpaperOpacity / 100
        asynchronous: true
        cache: true
        smooth: true
        mipmap: true
        layer.enabled: App.uiWallpaperBlur > 0 || App.uiWallpaperBrightness < 100
        layer.effect: MultiEffect {
            blurEnabled: true
            blur: App.uiWallpaperBlur / 100
            blurMax: 64
            autoPaddingEnabled: false
        }
        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 200 : 0 } }

        Rectangle {
            anchors.fill: parent
            color: "black"
            visible: App.uiWallpaperBrightness < 100
            opacity: (100 - App.uiWallpaperBrightness) / 100
            Behavior on opacity { NumberAnimation { duration: Theme.animations ? 200 : 0 } }
        }
    }

    readonly property string iconFont: "Segoe Fluent Icons"
    property bool navExpanded: false
    readonly property bool railCollapsed: !navExpanded
    property real railVisualWidth: railCollapsed ? Theme.railWidthCompact : Theme.railWidth
    property bool railAnimating: false
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

    function hasSavedWindowPosition() {
        return App.windowX !== -1 && App.windowY !== -1;
    }

    function clampToVirtualDesktop() {
        var vx = Number(Screen.virtualX);
        var vy = Number(Screen.virtualY);
        var vw = Number(Screen.virtualWidth);
        var vh = Number(Screen.virtualHeight);
        if (!isFinite(vx) || !isFinite(vy) || !isFinite(vw) || !isFinite(vh) || vw <= 0 || vh <= 0)
            return;

        var minVisibleX = Math.min(96, Math.max(32, win.width / 4));
        var minVisibleY = Math.min(96, Math.max(32, win.height / 4));
        var minX = vx - win.width + minVisibleX;
        var maxX = vx + vw - minVisibleX;
        var minY = vy;
        var maxY = vy + vh - minVisibleY;
        win.x = Math.round(Math.max(minX, Math.min(maxX, win.x)));
        win.y = Math.round(Math.max(minY, Math.min(maxY, win.y)));
    }

    function restoreSavedWindowPosition() {
        if (!hasSavedWindowPosition())
            return;
        win.x = App.windowX;
        win.y = App.windowY;
        clampToVirtualDesktop();
    }

    Component.onCompleted: {
        restoreSavedWindowPosition();
        Theme.accent = Qt.binding(function() { return App.accentColor; });
        Theme.dark = Qt.binding(function() { return win.resolveDark(); });
        Theme.density = Qt.binding(function() { return App.uiDensity; });
        Theme.cornerRadius = Qt.binding(function() { return App.uiCornerRadius; });
        Theme.animations = Qt.binding(function() { return App.uiAnimations; });
        Theme.backdrop = Qt.binding(function() { return App.uiBackdrop; });
        Theme.transparencyStrength = Qt.binding(function() { return App.uiTransparencyStrength; });
        Theme.wallpaperActive = Qt.binding(function() { return App.uiWallpaper !== ""; });
        Theme.fontScale = Qt.binding(function() { return App.uiFontScale / 100; });
        Theme.preset = Qt.binding(function() { return App.uiThemePreset; });
        Theme.baseTint = Qt.binding(function() { return App.uiBaseTint; });
        Theme.backdropAvailable = Qt.binding(function() { return App.uiBackdropAvailable; });
    }

    Timer {
        id: saveGeometryTimer
        interval: 500
        repeat: false
        onTriggered: {
            if (win.visibility === Window.Windowed) {
                App.saveWindowGeometry(win.width, win.height, win.x, win.y)
            }
        }
    }

    onWidthChanged: if (win.visibility === Window.Windowed) saveGeometryTimer.restart()
    onHeightChanged: if (win.visibility === Window.Windowed) saveGeometryTimer.restart()
    onXChanged: if (win.visibility === Window.Windowed) saveGeometryTimer.restart()
    onYChanged: if (win.visibility === Window.Windowed) saveGeometryTimer.restart()

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
    property int preloadCursor: 0
    property var preloadOrder: [1, 2, 3, 4, 5, 6, 7, 8, 9]
    property var pendingConfigDropUrls: []

    function queueConfigDrop(urls) {
        var queued = [];
        for (var i = 0; urls && i < urls.length; ++i)
            queued.push(urls[i]);
        if (queued.length === 0)
            return;
        pendingConfigDropUrls = queued;
        currentIndex = 1;
        nodesLoader.preloadRequested = true;
        if (nodesLoader.ready)
            Qt.callLater(finishConfigDrop);
    }

    function finishConfigDrop() {
        if (!nodesLoader.ready || pendingConfigDropUrls.length === 0)
            return;
        var urls = pendingConfigDropUrls;
        pendingConfigDropUrls = [];
        App.importNodeFiles(urls);
    }

    function beginBackgroundPageWarmup() {
        if (!backgroundPreloadTimer.running)
            backgroundPreloadTimer.start()
    }

    function preloadPage(index) {
        if (index === 1)
            nodesLoader.preloadRequested = true;
        else if (index === 2)
            routingLoader.preloadRequested = true;
        else if (index === 3)
            configsLoader.preloadRequested = true;
        else if (index === 4)
            zapretLoader.preloadRequested = true;
        else if (index === 5)
            logsLoader.preloadRequested = true;
        else if (index === 6)
            historyLoader.preloadRequested = true;
        else if (index === 7)
            updatesLoader.preloadRequested = true;
        else if (index === 8)
            settingsLoader.preloadRequested = true;
        else if (index === 9)
            aboutLoader.preloadRequested = true;
    }

    Timer {
        id: backgroundPreloadTimer
        interval: 120
        repeat: true
        running: false
        onTriggered: {
            if (win.preloadCursor >= win.preloadOrder.length) {
                stop();
                return;
            }
            win.preloadPage(win.preloadOrder[win.preloadCursor]);
            win.preloadCursor += 1;
        }
    }

    Connections {
        target: App
        function onSettingsChanged() {
            if (App.compactMode && [3, 5, 6].indexOf(win.currentIndex) !== -1)
                win.currentIndex = 0
        }
    }

    Item {
        anchors.fill: parent
        anchors.topMargin: titleBar.height

        // ── navigation pane ──────────────────────
        Rectangle {
            id: railClip
            width: win.railVisualWidth
            height: parent.height
            clip: true
            color: Theme.chromePanel
            z: 2

            Flickable {
                id: navFlick
                anchors.fill: parent
                anchors.topMargin: 8
                anchors.bottomMargin: 14
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                flickableDirection: Flickable.VerticalFlick // rail content must not be draggable sideways
                contentWidth: Theme.railWidth
                contentHeight: Math.max(height, railColumn.implicitHeight)
                ScrollBar.vertical: navScroll

                FluentScrollBar {
                    id: navScroll
                    parent: navFlick
                    x: 0
                    y: 0
                    height: navFlick.height
                    policy: railColumn.implicitHeight > railClip.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
                    thicknessNormal: 2
                    thicknessHover: 4
                }

                ColumnLayout {
                    id: railColumn
                    width: Theme.railWidth
                    height: Math.max(navFlick.height, implicitHeight)
                    spacing: 2

                    Item {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.round(40 * Theme.fontScale)
                        Rectangle {
                            x: 6
                            y: 0
                            width: win.railCollapsed ? Math.round(38 * Theme.fontScale) : Math.max(Math.round(38 * Theme.fontScale), parent.width - 12)
                            height: parent.height
                            radius: Theme.radiusSmall
                            color: burgerHover.hovered ? Theme.cardHover : "transparent"
                            Behavior on color { ColorAnimation { duration: Theme.animations ? 130 : 0; easing.type: Theme.easeStandard } }
                            Behavior on width { NumberAnimation { duration: Theme.animations ? 170 : 0; easing.type: Theme.easeEmphasized } }
                            Text {
                                text: "\uE700"  // GlobalNavButton
                                font.family: win.iconFont
                                font.pixelSize: Math.round(16 * Theme.fontScale)
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

                    Item { Layout.fillWidth: true; Layout.fillHeight: true; Layout.minimumHeight: 14 }

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
                anchors.fill: parent
                color: Theme.chromePanel
            }

            Rectangle {
                id: contentWindow
                anchors.fill: parent
                anchors.rightMargin: -Theme.radius
                anchors.bottomMargin: -Theme.radius
                clip: true
                radius: Theme.radius
                color: App.uiWallpaper !== ""
                       ? (Theme.backdrop === "acrylic"
                           ? (Theme.dark ? Qt.rgba(0, 0, 0, 0.32) : Qt.rgba(1, 1, 1, 0.28))
                           : (Theme.dark ? Qt.rgba(1, 1, 1, 0.045) : Qt.rgba(1, 1, 1, 0.18)))
                       : Theme.contentPanel
                border.width: 1
                border.color: Theme.divider

                Item {
                    id: pageStack
                    anchors.fill: parent
                    anchors.leftMargin: 24
                    anchors.topMargin: 20
                    anchors.rightMargin: 24 + Theme.radius
                    anchors.bottomMargin: 20 + Theme.radius
                    readonly property int slide: 6

                    LazyPageLoader {
                        id: dashboardLoader
                        current: win.currentIndex === 0
                        loadAsynchronously: false
                        pageComponent: Component { DashboardPage { anchors.fill: parent } }
                    }
                    LazyPageLoader {
                        id: nodesLoader
                        current: win.currentIndex === 1
                        pageComponent: Component { NodesPage { anchors.fill: parent } }
                        onReadyChanged: if (ready) Qt.callLater(win.finishConfigDrop)
                    }
                    LazyPageLoader {
                        id: routingLoader
                        current: win.currentIndex === 2
                        pageComponent: Component { RoutingPage { anchors.fill: parent } }
                    }
                    LazyPageLoader {
                        id: configsLoader
                        current: win.currentIndex === 3
                        pageComponent: Component { ConfigsPage { anchors.fill: parent } }
                    }
                    LazyPageLoader {
                        id: zapretLoader
                        current: win.currentIndex === 4
                        pageComponent: Component { ZapretPage { anchors.fill: parent } }
                    }
                    LazyPageLoader {
                        id: logsLoader
                        current: win.currentIndex === 5
                        pageComponent: Component { LogsPage { anchors.fill: parent } }
                    }
                    LazyPageLoader {
                        id: historyLoader
                        current: win.currentIndex === 6
                        pageComponent: Component { HistoryPage { anchors.fill: parent } }
                    }
                    LazyPageLoader {
                        id: updatesLoader
                        current: win.currentIndex === 7
                        pageComponent: Component { UpdatesPage { anchors.fill: parent } }
                    }
                    LazyPageLoader {
                        id: settingsLoader
                        current: win.currentIndex === 8
                        pageComponent: Component { SettingsPage { anchors.fill: parent } }
                    }
                    LazyPageLoader {
                        id: aboutLoader
                        current: win.currentIndex === 9
                        pageComponent: Component { AboutPage { anchors.fill: parent } }
                    }
                }
            }
        }
    }

    TitleBar {
        id: titleBar
        win: win
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right

        z: 50
    }

    ToastHost {}

    function hasSupportedConfigDrop(urls) {
        if (!urls)
            return false;
        for (var i = 0; i < urls.length; ++i) {
            var value = String(urls[i]).toLowerCase().split("?", 1)[0].split("#", 1)[0];
            if (/\.(conf|txt|json|ya?ml)$/.test(value))
                return true;
        }
        return false;
    }

    DropArea {
        id: configDropArea
        anchors.fill: parent
        z: 9000
        enabled: !App.locked

        onEntered: function(drag) {
            drag.accepted = win.hasSupportedConfigDrop(drag.urls);
        }
        onDropped: function(drop) {
            if (!win.hasSupportedConfigDrop(drop.urls)) {
                drop.accepted = false;
                return;
            }
            win.queueConfigDrop(drop.urls);
            drop.acceptProposedAction();
        }

        Rectangle {
            anchors.fill: parent
            visible: configDropArea.containsDrag
            color: Theme.dark ? Qt.rgba(0.04, 0.05, 0.07, 0.88) : Qt.rgba(0.96, 0.97, 0.99, 0.9)
            border.width: 2
            border.color: Theme.accent

            ColumnLayout {
                anchors.centerIn: parent
                spacing: 12
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "\uE8B7"
                    color: Theme.accent
                    font.family: win.iconFont
                    font.pixelSize: 42
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: I18n.t("Отпустите файлы для импорта")
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontStrong
                    font.weight: Font.DemiBold
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: ".conf  .txt  .json  .yaml  .yml"
                    color: Theme.textMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                }
            }
        }
    }

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
