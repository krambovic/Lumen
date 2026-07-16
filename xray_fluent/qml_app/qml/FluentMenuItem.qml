import QtQuick
import QtQuick.Controls.Universal
import "."

MenuItem {
    id: control
    implicitHeight: 32
    leftPadding: 12
    rightPadding: 12

    contentItem: Text {
        text: control.text
        color: control.enabled ? Theme.text : Theme.textFaint
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontNormal
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        radius: 6
        color: control.highlighted ? Theme.cardHover : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.animations ? 90 : 0 } }
    }
}
