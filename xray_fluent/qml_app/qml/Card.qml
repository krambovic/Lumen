import QtQuick
import "."

// A rounded surface used to group content, matching qfluentwidgets' CardWidget:
// a flat layer fill with a hairline border and a slightly darker bottom edge
// (the subtle Fluent "card stroke"). No drop shadow — Fluent cards sit flush on
// the Mica background, which also keeps everything cheap to composite on the GPU.
//
// Hover "attention" (ported from qfluentwidgets CardWidget.isHover): on pointer
// hover ONLY the surface fill lightens a touch — no scaling, no lift, no accent
// border — so the focused card gently stands out without disturbing layout,
// clipping decorative edges, or tinting its text/contents. Enabled by default;
// pass `hoverable: false` to opt out.
Item {
    id: root
    default property alias content: container.data
    property int padding: Theme.spacingLarge
    property color color: Theme.card
    property bool hoverable: true
    property alias radius: bg.radius

    // `container` is a plain Item, whose own implicitWidth/Height are always 0.
    // Size the card to its actual content child (e.g. the page's ColumnLayout)
    // instead — otherwise the card collapses to padding*2 and siblings overlap.
    readonly property Item _contentItem: container.children.length > 0 ? container.children[0] : null
    implicitWidth: (_contentItem ? _contentItem.implicitWidth : 0) + padding * 2
    implicitHeight: (_contentItem ? _contentItem.implicitHeight : 0) + padding * 2

    readonly property bool _hot: root.hoverable && hover.hovered

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: Theme.radius
        // Subtle surface-only attention: lighten the fill a hair on hover.
        color: root._hot ? Theme.cardHover : root.color
        border.width: 1
        // Hairline border stays constant — no accent glow on hover.
        border.color: Theme.borderSolid
        Behavior on color { ColorAnimation { duration: 120 } }

        // Fluent's slightly darker bottom stroke, faked with a 1px sliver that
        // follows the rounded corners.
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 1
            height: parent.radius
            radius: parent.radius
            color: "transparent"
            border.width: 1
            border.color: Theme.borderBottom
            // only the bottom edge should read; clip the top of this sliver
            visible: !Theme.dark
        }

        HoverHandler { id: hover; enabled: root.hoverable }
    }

    Item {
        id: container
        anchors.fill: parent
        anchors.margins: root.padding
    }
}