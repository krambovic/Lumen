import QtQuick
import QtQuick.Layouts
import "."

// One entry in the left navigation pane, styled after the qfluentwidgets
// NavigationInterface item: icon (+ label), a soft accent fill when selected
// and a short rounded accent bar on the leading edge.
Item {
    id: root
    property string label: ""
    property string glyph: "\uE700"
    property string iconFont: "Segoe Fluent Icons"
    property bool selected: false
    property bool compact: false
    signal clicked()

    implicitHeight: 40
    implicitWidth: parent ? parent.width : 200

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: 6
        anchors.rightMargin: 6
        radius: Theme.radiusSmall
        color: root.selected ? Theme.accentSoft
               : tap.hovered ? Theme.cardHover : "transparent"
        Behavior on color { ColorAnimation { duration: 110 } }

        Rectangle {  // selection indicator bar
            visible: root.selected
            width: 3; radius: 1.5
            color: Theme.accent
            anchors.left: parent.left
            anchors.leftMargin: 3
            anchors.verticalCenter: parent.verticalCenter
            height: parent.height * 0.5
        }

        RowLayout {
            anchors.fill: parent
            // Pin the icon at a fixed offset equal to its centred position in
            // the collapsed rail. Collapsing/expanding then only grows or trims
            // the trailing space and shows/hides the label - the icon itself
            // never moves, so it no longer snaps to the centre of the still-wide
            // rail on toggle (the cause of the jump).
            anchors.leftMargin: 11
            anchors.rightMargin: 12
            spacing: 12

            Text {
                text: root.glyph
                font.family: root.iconFont
                font.pixelSize: 16
                color: root.selected ? Theme.text : Theme.textMuted
                Layout.alignment: Qt.AlignVCenter
                horizontalAlignment: Text.AlignLeft
            }
            Text {
                text: root.label
                visible: !root.compact
                Layout.fillWidth: true
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                font.weight: root.selected ? Font.DemiBold : Font.Normal
                color: root.selected ? Theme.text : Theme.textMuted
                elide: Text.ElideRight
            }
        }

        HoverHandler { id: tap }
        TapHandler { onTapped: root.clicked() }
    }
}
