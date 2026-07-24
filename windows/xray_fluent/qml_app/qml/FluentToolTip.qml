import QtQuick
import QtQuick.Controls.Universal
import "."

ToolTip {
    id: control
    delay: 450
    timeout: 6000
    margins: 8
    x: parent ? Math.round((parent.width - implicitWidth) / 2) : 0
    // Keep toolbar hints immediately above their button (WinUI style).  The
    // popup engine will flip them when there is no room near the window edge.
    y: parent ? -implicitHeight - 4 : 0
    leftPadding: 12
    rightPadding: 12
    topPadding: 8
    bottomPadding: 8

    contentItem: Text {
        text: control.text
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontNormal
        wrapMode: Text.WordWrap
        maximumLineCount: 4
        elide: Text.ElideRight
    }

    background: Rectangle {
        color: Theme.flyout
        radius: 8
        border.width: 1
        border.color: Theme.flyoutBorder
    }

    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0.0; to: 1.0; duration: Theme.animations ? 100 : 0 }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; from: 1.0; to: 0.0; duration: Theme.animations ? 70 : 0 }
    }
}
