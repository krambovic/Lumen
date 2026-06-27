import QtQuick
import QtQuick.Window
import App 1.0
import "."

Item {
    id: bar
    objectName: "qwkTitleBar"
    property var win
    readonly property bool _maxified: win && (win.visibility === Window.FullScreen
                                            || win.visibility === Window.Maximized)
    height: Math.round(34 * Theme.fontScale) + (_maxified ? 2 : 0)

    function toggleMax() {
        if (!win) return
        if (win.visibility === Window.Maximized) win.showNormal()
        else win.showMaximized()
    }

    component WinBtn: Item {
        id: wb
        property string glyph: ""
        property bool danger: false
        signal activated()
        width: Math.round(44 * Theme.fontScale)
        height: parent ? parent.height : 34
        Rectangle {
            anchors.fill: parent
            color: wbHover.hovered ? (wb.danger ? "#E81123" : Theme.cardHover) : "transparent"
        }
        Text {
            anchors.centerIn: parent
            text: wb.glyph
            font.family: "Segoe Fluent Icons"
            font.pixelSize: Math.round(10 * Theme.fontScale)
            color: (wb.danger && wbHover.hovered) ? "white" : Theme.textMuted
        }
        HoverHandler { id: wbHover }
        TapHandler { onTapped: wb.activated() }
    }

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        text: App.appName
        color: Theme.textMuted
        font.family: Theme.fontFamily
        font.pixelSize: Math.round(14 * Theme.fontScale)
    }

    Row {
        anchors.right: parent.right
        anchors.top: parent.top
        height: parent.height
        spacing: 0

        WinBtn {
            objectName: "qwkMinBtn"
            glyph: "\uE921"  // minimize
            onActivated: if (bar.win) bar.win.showMinimized()
        }
        WinBtn {
            objectName: "qwkMaxBtn"
            glyph: (bar.win && bar.win.visibility === Window.Maximized) ? "\uE923" : "\uE922" // maximize/restore
            onActivated: bar.toggleMax()
        }
        WinBtn {
            objectName: "qwkCloseBtn"
            glyph: "\uE8BB"  // close
            danger: true
            onActivated: if (bar.win) bar.win.close()
        }
    }
}
