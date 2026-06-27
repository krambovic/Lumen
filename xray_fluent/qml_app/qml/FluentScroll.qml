import QtQuick
import QtQuick.Effects
import "."
Item {
    id: root

    default property alias content: contentHost.data
    property alias contentY: flick.contentY
    property alias contentHeight: flick.contentHeight
    property alias contentWidth: flick.contentWidth
    property alias moving: flick.moving
    property alias flicking: flick.flicking
    property real wheelStep: 92
    property int glide: 540
    property bool barForcedVisible: false
    property bool roundedClip: true
    property real clipRadius: Theme.radius

    readonly property real maxY: Math.max(0, flick.contentHeight - flick.height)
    readonly property bool scrollable: maxY > 1
    readonly property bool barShown: scrollable && (barForcedVisible || barHover.hovered || thumbDrag.drag.active)

    clip: false

    function flashBar() {
        if (!scrollable)
            return
        barForcedVisible = true
        hideBarTimer.restart()
    }

    Timer {
        id: hideBarTimer
        interval: 720
        repeat: false
        onTriggered: root.barForcedVisible = false
    }

    Flickable {
        id: flick
        anchors.fill: parent
        clip: !root.roundedClip
        layer.enabled: root.roundedClip && root.clipRadius > 0
        layer.smooth: true
        layer.effect: MultiEffect {
            maskEnabled: true
            maskThresholdMin: 0.5
            maskSpreadAtMin: 1.0
            maskSource: ShaderEffectSource {
                hideSource: true
                sourceItem: Rectangle {
                    width: flick.width
                    height: flick.height
                    radius: root.clipRadius
                    color: "black"
                }
            }
        }
        boundsBehavior: Flickable.StopAtBounds
        boundsMovement: Flickable.StopAtBounds
        contentWidth: width
        contentHeight: contentHost.childrenRect.height
        flickDeceleration: 5000
        maximumFlickVelocity: 3000
        pixelAligned: true

        Item {
            id: contentHost
            width: flick.width
            height: childrenRect.height
        }

        NumberAnimation {
            id: scrollAnim
            target: flick
            property: "contentY"
            duration: root.glide
            easing.type: Easing.OutQuint
            onRunningChanged: if (running) root.flashBar()
        }

        onContentYChanged: if (moving || flicking) root.flashBar()
        onHeightChanged: flick.contentY = Math.max(0, Math.min(root.maxY, flick.contentY))
        onContentHeightChanged: flick.contentY = Math.max(0, Math.min(root.maxY, flick.contentY))

        WheelHandler {
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: (ev) => {
                var maxY = root.maxY
                if (maxY <= 0) { ev.accepted = true; return }
                if (ev.pixelDelta.y !== 0) {
                    scrollAnim.stop()
                    flick.contentY = Math.max(0, Math.min(maxY, flick.contentY - ev.pixelDelta.y))
                } else {
                    var base = scrollAnim.running ? scrollAnim.to : flick.contentY
                    var target = Math.max(0, Math.min(maxY, base - (ev.angleDelta.y / 120) * root.wheelStep))
                    scrollAnim.to = target
                    scrollAnim.restart()
                }
                root.flashBar()
                ev.accepted = true
            }
        }
    }

    Item {
        id: barTrack
        width: 12
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.rightMargin: -18
        visible: root.scrollable
        opacity: root.barShown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 160 : 0; easing.type: Theme.easeStandard } }

        HoverHandler { id: barHover }

        Rectangle {
            id: thumb
            readonly property real ratio: flick.contentHeight <= 0 ? 1 : Math.min(1, flick.height / flick.contentHeight)
            readonly property real minThumb: 44
            width: thumbDrag.drag.active || barHover.hovered ? 8 : 4
            height: Math.max(minThumb, barTrack.height * ratio)
            x: Math.round((barTrack.width - width) / 2)
            y: root.maxY <= 0 ? 0 : (barTrack.height - height) * (flick.contentY / root.maxY)
            radius: width / 2
            color: thumbDrag.drag.active ? Theme.textMuted : Theme.textFaint
            opacity: thumbDrag.drag.active ? 0.9 : 0.72
            Behavior on width { NumberAnimation { duration: Theme.animations ? 120 : 0; easing.type: Easing.OutCubic } }

            MouseArea {
                id: thumbDrag
                anchors.fill: parent
                hoverEnabled: true
                drag.target: parent
                drag.axis: Drag.YAxis
                drag.minimumY: 0
                drag.maximumY: Math.max(0, barTrack.height - thumb.height)
                preventStealing: true

                onPressed: {
                    scrollAnim.stop()
                    root.flashBar()
                }
                onPositionChanged: {
                    if (!drag.active)
                        return
                    var denom = Math.max(1, barTrack.height - thumb.height)
                    flick.contentY = Math.max(0, Math.min(root.maxY, (thumb.y / denom) * root.maxY))
                    root.flashBar()
                }
                onReleased: root.flashBar()
                onCanceled: root.flashBar()
            }
        }
    }
}
