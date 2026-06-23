import QtQuick

Item {
    id: root

    property bool current: false
    property bool loaded: false
    property bool loadAsynchronously: true
    property bool preloadRequested: false
    property Component pageComponent
    property int slide: 6

    anchors.fill: parent
    visible: opacity > 0
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

    Loader {
        anchors.fill: parent
        active: root.loaded
        asynchronous: root.loadAsynchronously
        sourceComponent: root.pageComponent
    }
}
