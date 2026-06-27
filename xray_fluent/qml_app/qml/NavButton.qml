import QtQuick
import "."

Item {
    id: root
    property string label: ""
    property string glyph: "\uE700"
    property string iconFont: "Segoe Fluent Icons"
    property bool selected: false
    property bool compact: false
    property bool badge: false
    signal clicked()

    implicitHeight: Math.round(40 * Theme.fontScale)
    implicitWidth: parent ? parent.width : 200

    Rectangle {
        id: bg
        x: 6
        y: 0
        width: root.compact ? 38 : Math.max(38, root.width - 12)
        height: parent.height
        radius: Theme.radiusSmall
        color: root.selected ? Theme.accentSoft
               : tap.hovered ? Theme.cardHover : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.animations ? 130 : 0; easing.type: Theme.easeStandard } }
        Behavior on width { NumberAnimation { duration: Theme.animations ? 170 : 0; easing.type: Theme.easeEmphasized } }

        Rectangle {
            width: 3; radius: 1.5
            color: Theme.accent
            anchors.left: parent.left
            anchors.leftMargin: 3
            anchors.verticalCenter: parent.verticalCenter
            height: root.selected ? parent.height * 0.5 : 0
            opacity: root.selected ? 1 : 0
            Behavior on height { NumberAnimation { duration: Theme.animations ? 210 : 0; easing.type: Theme.easeEmphasized } }
            Behavior on opacity { NumberAnimation { duration: Theme.animations ? 150 : 0; easing.type: Theme.easeStandard } }
        }

        Text {
            id: icon
            text: root.glyph
            font.family: root.iconFont
            font.pixelSize: Math.round(16 * Theme.fontScale)
            color: root.selected ? Theme.text : Theme.textMuted
            anchors.left: parent.left
            anchors.leftMargin: 11
            anchors.verticalCenter: parent.verticalCenter
            width: 18
            horizontalAlignment: Text.AlignLeft
            Behavior on color { ColorAnimation { duration: Theme.animations ? 140 : 0; easing.type: Theme.easeStandard } }

            Rectangle {
                visible: root.badge
                width: 8; height: 8; radius: 4
                color: "#E81123"
                border.width: 1
                border.color: Theme.flyout
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.rightMargin: -3
                anchors.topMargin: -1
            }
        }

        Text {
            id: labelText
            text: root.label
            anchors.left: parent.left
            anchors.leftMargin: 41
            anchors.right: parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            opacity: root.compact ? 0 : 1
            visible: !root.compact
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontNormal
            font.weight: root.selected ? Font.DemiBold : Font.Normal
            color: root.selected ? Theme.text : Theme.textMuted
            elide: Text.ElideRight
            clip: true
        }

        HoverHandler { id: tap }
        TapHandler { onTapped: root.clicked() }
    }
}