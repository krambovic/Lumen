import QtQuick
import "."

Item {
    id: root
    default property alias content: container.data
    property int padding: Theme.spacingLarge
    property color color: Theme.card
    property bool hoverable: true
    property alias radius: bg.radius
    readonly property Item _contentItem: container.children.length > 0 ? container.children[0] : null
    implicitWidth: (_contentItem ? _contentItem.implicitWidth : 0) + padding * 2
    implicitHeight: (_contentItem ? _contentItem.implicitHeight : 0) + padding * 2

    readonly property bool _hot: root.hoverable && hover.hovered

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: Theme.radius
        color: root.color
        border.width: 1
        border.color: Theme.borderSolid
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: Theme.dark ? Qt.rgba(1, 1, 1, 0.035) : Theme.cardHover
            opacity: root._hot ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 120 } }
        }

        Item {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: bg.radius + 1
            clip: true
            visible: !Theme.dark

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: bg.height
                radius: bg.radius
                color: "transparent"
                border.width: 1
                border.color: Theme.borderBottom
            }
        }

        HoverHandler { id: hover; enabled: root.hoverable }
    }

    Item {
        id: container
        anchors.fill: parent
        anchors.margins: root.padding
    }
}