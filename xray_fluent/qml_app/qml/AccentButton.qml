import QtQuick
import QtQuick.Controls.Universal
import "."

// Themed push button matching qfluentwidgets buttons. Three variants via `kind`:
//   "accent"  - filled accent (PrimaryPushButton)
//   "ghost"   - standard subtle fill + hairline border (PushButton)
//   "danger"  - filled red (destructive)
//
// Set `iconOnly: true` for a compact square icon button (Fluent CommandBar /
// ToolButton look): the text label is hidden and shown instead as a tooltip
// after hovering the icon, mirroring the original toolbar buttons.
Button {
    id: control
    property string kind: "accent"
    readonly property bool standard: kind === "ghost"
    // Foreground (glyph + label) colour: dark text on the subtle ghost fill,
    // light text on the filled accent/danger backgrounds.
    readonly property color fgColor: standard ? Theme.text : Theme.accentText
    // Optional leading Fluent glyph (e.g. ▶ Play / ⏸ Pause on the toggle
    // button), mirroring qfluentwidgets buttons built with a FluentIcon.
    property string glyph: ""
    // Compact, label-less square icon button with a hover tooltip.
    property bool iconOnly: false
    // Tooltip text (defaults to the button text).
    property string tip: text

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontNormal
    font.weight: standard ? Font.Normal : Font.DemiBold
    implicitHeight: Theme.controlHeight
    implicitWidth: iconOnly ? Theme.controlHeight : Math.max(implicitContentWidth + leftPadding + rightPadding, 0)
    leftPadding: iconOnly ? 0 : 14
    rightPadding: iconOnly ? 0 : 14
    topPadding: 0
    bottomPadding: 0

    // Subtle Fluent press feedback.
    scale: down ? 0.97 : 1.0
    Behavior on scale { NumberAnimation { duration: 90; easing.type: Easing.OutQuad } }

    // Hover tooltip — primarily for the icon-only variant, but also shown for
    // any button that has a glyph but no visible label.
    ToolTip.visible: control.hovered && control.tip.length > 0 && (control.iconOnly || control.text.length === 0)
    ToolTip.text: control.tip
    ToolTip.delay: 450

    contentItem: Item {
        implicitWidth: rowInner.implicitWidth
        implicitHeight: rowInner.implicitHeight
        Row {
            id: rowInner
            anchors.centerIn: parent
            spacing: 8
            Text {
                visible: control.glyph.length > 0
                text: control.glyph
                font.family: "Segoe Fluent Icons"
                font.pixelSize: Theme.fontNormal
                color: control.fgColor
                opacity: control.enabled ? 1.0 : 0.45
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                visible: !control.iconOnly && control.text.length > 0
                text: control.text
                font: control.font
                color: control.fgColor
                opacity: control.enabled ? 1.0 : 0.45
                verticalAlignment: Text.AlignVCenter
                anchors.verticalCenter: parent.verticalCenter
                elide: Text.ElideRight
            }
        }
    }

    background: Rectangle {
        radius: Theme.radiusSmall
        border.width: control.standard ? 1 : 0
        border.color: Theme.borderSolid
        color: {
            if (control.standard) {
                if (!control.enabled) return "transparent";
                if (control.down) return Theme.controlFillPressed;
                return control.hovered ? Theme.controlFillHover : Theme.controlFill;
            }
            // Slightly deepen the danger red so the white label stays legible.
            var base = control.kind === "danger" ? Qt.darker(Theme.danger, 1.22) : Theme.accent;
            if (!control.enabled) return Qt.rgba(base.r, base.g, base.b, 0.4);
            if (control.down) return Theme.dark ? Qt.lighter(base, 1.18) : Qt.darker(base, 1.12);
            return control.hovered ? (Theme.dark ? Qt.lighter(base, 1.10) : Qt.darker(base, 1.06)) : base;
        }
        Behavior on color { ColorAnimation { duration: 110 } }
    }
}
