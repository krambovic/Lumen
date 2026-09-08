import QtQuick

Item {
    id: root

    property bool current: false
    property bool loaded: false
    property bool loadAsynchronously: true
    property bool preloadRequested: false
    property Component pageComponent
    property int slide: 6
    readonly property bool ready: pageLoader.status === Loader.Ready

    readonly property bool laidOut: current || opacity > 0

    x: 0
    y: 0
    // Warm pages at their real viewport size. A zero-size preload leaves
    // virtual lists unlaid-out when the user opens the page immediately.
    width: parent ? parent.width : 0
    height: parent ? parent.height : 0
    visible: laidOut
    opacity: current ? 1 : 0

    onCurrentChanged: {
        if (current)
            loaded = true
    }
    onPreloadRequestedChanged: {
        if (preloadRequested)
            loaded = true
    }
    Component.onCompleted: {
        if (current || preloadRequested)
            loaded = true
    }

    Behavior on opacity {
        NumberAnimation {
            duration: Theme.animations ? 220 : 0
            easing.type: Theme.easeStandard
        }
    }

    transform: Translate {
        y: root.current ? 0 : root.slide
        Behavior on y {
            NumberAnimation {
                duration: Theme.animations ? 260 : 0
                easing.type: Theme.easeEmphasized
            }
        }
    }

    Text {
        anchors.centerIn: parent
        width: Math.max(0, Math.min(parent.width - 24, 380))
        visible: root.current && !root.ready
        text: pageLoader.status === Loader.Error ? I18n.t("Не удалось открыть раздел") : I18n.t("Загрузка раздела…")
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        color: Theme.textMuted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontNormal
    }

    Loader {
        id: pageLoader
        anchors.fill: parent
        active: root.loaded
        // Background pages remain lazy/asynchronous. Opening a page promotes
        // that one load to completion instead of exposing an empty surface.
        asynchronous: root.loadAsynchronously && !root.current
        sourceComponent: root.pageComponent
    }
}
