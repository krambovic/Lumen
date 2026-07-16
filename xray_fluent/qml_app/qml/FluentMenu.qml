import QtQuick
import QtQuick.Controls.Universal
import "."

Menu {
    id: control
    padding: 5
    margins: 8

    background: Rectangle {
        color: Theme.flyout
        radius: 8
        border.width: 1
        border.color: Theme.flyoutBorder
    }

    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0.0; to: 1.0; duration: Theme.animations ? 90 : 0 }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; from: 1.0; to: 0.0; duration: Theme.animations ? 70 : 0 }
    }
}
