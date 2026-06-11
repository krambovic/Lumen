import QtQuick
import QtQuick.Controls.Universal
import "."

// Shared Windows 11-style scrollbar used everywhere (FluentScroll + every inner
// ListView / ScrollView). Thin, rounded, NO grey square track, and — like the
// original — it stays faintly visible whenever there is something to scroll,
// brightening and thickening on hover / drag. Works for both orientations, so
// it can be dropped straight into any ScrollBar.vertical / ScrollBar.horizontal
// slot.
ScrollBar {
    id: bar
    policy: ScrollBar.AsNeeded
    padding: 2
    minimumSize: 0.12
    // Не показываем полосу, если страница и так помещается целиком (допуск на округление в 1–2px).
    visible: bar.size < 0.999

    readonly property bool isVertical: orientation === Qt.Vertical
    readonly property bool big: bar.hovered || bar.pressed
    readonly property int thickness: big ? 8 : 4

    contentItem: Rectangle {
        implicitWidth: bar.isVertical ? bar.thickness : 40
        implicitHeight: bar.isVertical ? 40 : bar.thickness
        radius: (bar.isVertical ? implicitWidth : implicitHeight) / 2
        color: bar.pressed ? Theme.textMuted : Theme.textFaint
        // Always slightly visible while scrollable; brighter while interacting.
        opacity: bar.active ? 0.9 : 0.38
        Behavior on opacity { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
        Behavior on implicitWidth { NumberAnimation { duration: 130; easing.type: Easing.OutCubic } }
        Behavior on implicitHeight { NumberAnimation { duration: 130; easing.type: Easing.OutCubic } }
    }

    background: Rectangle { color: "transparent" }
}
