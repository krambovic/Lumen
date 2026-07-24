import QtQuick
import QtQuick.Controls.Universal
import "."

ScrollBar {
    id: bar
    policy: ScrollBar.AsNeeded
    padding: 2
    minimumSize: 0.12
    visible: bar.size < 0.999

    readonly property bool isVertical: orientation === Qt.Vertical
    readonly property bool big: bar.hovered || bar.pressed
    property bool forcedVisible: false
    property int thicknessNormal: 4
    property int thicknessHover: 8
    readonly property int thickness: big ? thicknessHover : thicknessNormal

    function flash() {
        forcedVisible = true
        hideTimer.restart()
    }

    Timer {
        id: hideTimer
        interval: 650
        repeat: false
        onTriggered: bar.forcedVisible = false
    }

    contentItem: Rectangle {
        implicitWidth: bar.isVertical ? bar.thickness : 40
        implicitHeight: bar.isVertical ? 40 : bar.thickness
        radius: (bar.isVertical ? implicitWidth : implicitHeight) / 2
        color: bar.pressed ? Theme.textMuted : Theme.textFaint
        opacity: (bar.pressed || bar.forcedVisible || bar.active) ? 0.9 : 0.38
        Behavior on opacity { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
        Behavior on implicitWidth { NumberAnimation { duration: 130; easing.type: Easing.OutCubic } }
        Behavior on implicitHeight { NumberAnimation { duration: 130; easing.type: Easing.OutCubic } }
    }

    background: Rectangle { color: "transparent" }
}
