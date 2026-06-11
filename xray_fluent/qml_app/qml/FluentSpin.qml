import QtQuick
import QtQuick.Controls.Universal
import "."

// Windows 11 / WinUI-style SpinBox shared across the settings rows.
//
// The built-in Universal SpinBox uses square Windows-8 "Metro" side buttons
// with a flat field. This component gives it the rounded Fluent look that
// matches FluentCombo / StyledField:
//   * rounded field with a hairline border that turns accent on focus
//   * centered value text
//   * chevron-glyph steppers (‹ / ›) that highlight on hover/press
SpinBox {
    id: control

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontNormal
    implicitHeight: Theme.controlHeight
    implicitWidth: 130
    editable: true
    opacity: enabled ? 1.0 : 0.5

    leftPadding: 34
    rightPadding: 34
    topPadding: 0
    bottomPadding: 0

    contentItem: TextInput {
        text: control.displayText
        font: control.font
        color: Theme.text
        selectionColor: Theme.accent
        selectedTextColor: Theme.accentText
        horizontalAlignment: Qt.AlignHCenter
        verticalAlignment: Qt.AlignVCenter
        readOnly: !control.editable
        validator: control.validator
        inputMethodHints: Qt.ImhFormattedNumbersOnly
    }

    down.indicator: Rectangle {
        x: 4
        y: (control.height - height) / 2
        width: 26
        height: control.height - 8
        radius: Theme.radiusSmall
        color: control.down.pressed ? Theme.controlFillPressed
               : (control.down.hovered ? Theme.controlFillHover : "transparent")
        Text {
            anchors.centerIn: parent
            text: "\uE76B" // ChevronLeft
            font.family: "Segoe Fluent Icons"
            font.pixelSize: 11
            color: control.enabled ? Theme.textMuted : Theme.textFaint
        }
    }

    up.indicator: Rectangle {
        x: control.width - width - 4
        y: (control.height - height) / 2
        width: 26
        height: control.height - 8
        radius: Theme.radiusSmall
        color: control.up.pressed ? Theme.controlFillPressed
               : (control.up.hovered ? Theme.controlFillHover : "transparent")
        Text {
            anchors.centerIn: parent
            text: "\uE76C" // ChevronRight
            font.family: "Segoe Fluent Icons"
            font.pixelSize: 11
            color: control.enabled ? Theme.textMuted : Theme.textFaint
        }
    }

    background: Rectangle {
        implicitWidth: 130
        radius: Theme.radiusSmall
        color: control.hovered && !control.activeFocus ? Theme.controlFillHover : Theme.controlFill
        border.width: 1
        border.color: control.activeFocus ? Theme.accent : Theme.borderSolid
        Behavior on color { ColorAnimation { duration: 110 } }
        Behavior on border.color { ColorAnimation { duration: 110 } }
    }
}
