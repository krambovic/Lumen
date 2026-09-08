import QtQuick
import QtQuick.Window
import "."

Item {
    id: root
    property string label: ""
    property string glyph: "\uE700"
    property string iconFont: "Segoe Fluent Icons"
    property string iconName: ""
    readonly property string outlinePath: {
        var paths = {
            "dashboard": "M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z",
            "servers": "M4 3h16v7H4z M4 14h16v7H4z M7 6h.01 M7 17h.01 M11 6h6 M11 17h6",
            "routing": "M5 4v10a5 5 0 0 0 5 5h9 M15 15l4 4-4 4 M5 4l-3 3 M5 4l3 3 M5 11h10a4 4 0 0 0 4-4V3",
            "configs": "M6 3h8l4 4v14H6z M14 3v5h5 M10 11l-2 3 2 3 M14 11l2 3-2 3",
            "zapret": "M12 2l8 4v6c0 5-8 10-8 10S4 17 4 12V6z M13 6l-5 7h4l-1 5 6-8h-4l1-4",
            "logs": "M3 4h18v16H3z M6 8l4 4-4 4 M12 16h5",
            "history": "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20 M12 6v6l4 2"
        };
        return paths[iconName] || "";
    }
    readonly property string outlineSource: {
        if (!outlinePath.length) return "";
        var ink = selected ? Theme.text : Theme.textMuted;
        var rgb = Math.round(ink.r * 255) + "," + Math.round(ink.g * 255) + "," + Math.round(ink.b * 255);
        var svg = '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">'
            + '<path d="' + outlinePath + '" fill="none" stroke="rgb(' + rgb + ')" stroke-opacity="' + ink.a
            + '" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round"/></svg>';
        return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
    }
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
        color: "transparent"
        Behavior on width { NumberAnimation { duration: Theme.animations ? 170 : 0; easing.type: Theme.easeEmphasized } }

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: root.selected ? Theme.accentSoft : Theme.navHover
            opacity: root.selected || tap.containsMouse ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.animations ? 130 : 0; easing.type: Theme.easeStandard } }
        }

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

        Image {
            id: outlineIcon
            visible: root.outlinePath.length > 0
            source: root.outlineSource
            anchors.left: parent.left
            anchors.leftMargin: 11
            anchors.verticalCenter: parent.verticalCenter
            width: Math.round(18 * Theme.fontScale)
            height: width
            sourceSize.width: Math.max(1, Math.ceil(width * Screen.devicePixelRatio))
            sourceSize.height: Math.max(1, Math.ceil(height * Screen.devicePixelRatio))
            fillMode: Image.PreserveAspectFit
            asynchronous: false
            cache: true
            smooth: true
            mipmap: true
        }

        Text {
            id: icon
            visible: root.outlinePath.length === 0
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

        MouseArea {
            id: tap
            anchors.fill: parent
            hoverEnabled: true
            onClicked: root.clicked()
        }
    }
}
