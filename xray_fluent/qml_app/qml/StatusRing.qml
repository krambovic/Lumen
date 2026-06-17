pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Shapes
import App 1.0
import "."

// State-machine status ring
Item {
    id: ring
    property bool active: false
    property bool busy: false
    property bool targetActive: active

    implicitWidth: 60
    implicitHeight: 60

    readonly property real ringSize: Math.min(width, height) - 8
    readonly property real lineW: 4
    readonly property color idleTrackColor: Theme.dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0, 0, 0, 0.10)
    readonly property color idleDotColor: Theme.dark ? Theme.textFaint : Qt.rgba(0, 0, 0, 0.36)
    readonly property bool visualTargetActive: targetActive
    readonly property color transitionColor: visualTargetActive ? Theme.warning : Theme.danger

    // Visual state
    property real arcSweep: active ? 360 : 0
    property color arcColor: active ? Theme.success : transitionColor
    property color dotColor: active ? Theme.success : idleDotColor
    property real dotScale: active ? 1.10 : 1.0
    property real spin: 0
    property bool settlingToTop: false
    property bool settleToConnected: false
    property real busyStartedAt: 0
    property int settleDuration: Theme.animations ? 520 : 0

    function syncThemeColors() {
        if (busy) {
            arcColor = transitionColor
            dotColor = transitionColor
            return
        }
        if (active) {
            arcSweep = 360
            arcColor = Theme.success
            dotColor = Theme.success
            dotScale = 1.10
        } else {
            arcSweep = 0
            arcColor = transitionColor
            dotColor = idleDotColor
            dotScale = 1.0
        }
    }

    function finishToConnected() {
        spinLoop.stop()
        settleSpin.stop()
        toTransition.stop()
        toIdle.stop()
        settlingToTop = true
        settleToConnected = true
        settleDuration = Theme.animations ? 520 : 0
        var current = spin
        if (current === 0)
            current = 0.01
        spin = current
        settleSpin.from = current
        settleSpin.to = Math.ceil(current / 360) * 360
        if (settleSpin.to === current)
            settleSpin.to = current + 360
        toConnected.restart()
        settleSpin.restart()
    }

    function finishToIdle() {
        spinLoop.stop()
        toConnected.stop()
        toTransition.stop()
        settlingToTop = true
        settleToConnected = false
        var current = spin
        var fastDisconnect = (Date.now() - busyStartedAt) < 450
        if (Math.abs(current) < 0.01)
            current = fastDisconnect ? 0 : -0.01
        spin = current
        settleSpin.from = current
        if (fastDisconnect) {
            settleSpin.to = Math.round(current / 360) * 360
            settleDuration = Theme.animations ? 220 : 0
        } else {
            settleSpin.to = Math.floor(current / 360) * 360
            if (settleSpin.to === current)
                settleSpin.to = current - 360
            settleDuration = Theme.animations ? 520 : 0
        }
        toIdle.restart()
        settleSpin.restart()
    }

    onBusyChanged: {
        if (busy) {
            settleSpin.stop()
            toConnected.stop()
            toIdle.stop()
            settlingToTop = false
            busyStartedAt = Date.now()
            spin = 0
            toTransition.restart()
            spinLoop.restart()
        } else {
            if (active)
                finishToConnected()
            else
                finishToIdle()
        }
    }

    onActiveChanged: {
        if (busy)
            return
        if (active)
            finishToConnected()
        else
            finishToIdle()
    }

    Connections {
        target: Theme
        function onDarkChanged() { ring.syncThemeColors() }
        function onAccentChanged() { ring.syncThemeColors() }
    }

    Component.onCompleted: {
        syncThemeColors()
        spin = 0
    }

    ParallelAnimation {
        id: toTransition
        NumberAnimation { target: ring; property: "arcSweep"; to: 216; duration: Theme.animations ? 620 : 0; easing.type: Easing.InOutCubic }
        ColorAnimation { target: ring; property: "arcColor"; to: ring.transitionColor; duration: Theme.animations ? 520 : 0; easing.type: Theme.easeStandard }
        ColorAnimation { target: ring; property: "dotColor"; to: ring.transitionColor; duration: Theme.animations ? 480 : 0; easing.type: Theme.easeStandard }
        NumberAnimation { target: ring; property: "dotScale"; to: 1.03; duration: Theme.animations ? 420 : 0; easing.type: Theme.easeStandard }
    }

    ParallelAnimation {
        id: toConnected
        NumberAnimation { target: ring; property: "arcSweep"; to: 360; duration: Theme.animations ? 520 : 0; easing.type: Easing.InOutCubic }
        ColorAnimation { target: ring; property: "arcColor"; to: Theme.success; duration: Theme.animations ? 460 : 0; easing.type: Theme.easeStandard }
        ColorAnimation { target: ring; property: "dotColor"; to: Theme.success; duration: Theme.animations ? 420 : 0; easing.type: Theme.easeStandard }
        NumberAnimation { target: ring; property: "dotScale"; to: 1.10; duration: Theme.animations ? 360 : 0; easing.type: Theme.easeStandard }
    }

    SequentialAnimation {
        id: toIdle
        ParallelAnimation {
            NumberAnimation { target: ring; property: "arcSweep"; to: 0; duration: Theme.animations ? 420 : 0; easing.type: Easing.InOutCubic }
            ColorAnimation { target: ring; property: "arcColor"; to: ring.transitionColor; duration: Theme.animations ? 180 : 0; easing.type: Theme.easeStandard }
            ColorAnimation { target: ring; property: "dotColor"; to: ring.idleDotColor; duration: Theme.animations ? 360 : 0; easing.type: Theme.easeStandard }
            NumberAnimation { target: ring; property: "dotScale"; to: 1.0; duration: Theme.animations ? 340 : 0; easing.type: Theme.easeStandard }
        }
    }

    NumberAnimation {
        id: spinLoop
        target: ring
        property: "spin"
        from: 0
        to: ring.visualTargetActive ? 360 : -360
        duration: 1100
        loops: Animation.Infinite
    }

    NumberAnimation {
        id: settleSpin
        target: ring
        property: "spin"
        duration: ring.settleDuration
        easing.type: Easing.OutCubic
        onStopped: {
            ring.spin = 0
            ring.settlingToTop = false
            ring.settleToConnected = false
            if (ring.active && !ring.busy) {
                ring.arcSweep = 360
                ring.arcColor = Theme.success
                ring.dotColor = Theme.success
                ring.dotScale = 1.10
            }
        }
    }

    Shape {
        id: track
        anchors.fill: parent
        layer.enabled: true
        layer.samples: 16
        layer.smooth: true
        ShapePath {
            fillColor: "transparent"
            strokeColor: ring.idleTrackColor
            strokeWidth: ring.lineW
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: ring.width / 2; centerY: ring.height / 2
                radiusX: ring.ringSize / 2; radiusY: ring.ringSize / 2
                startAngle: -90; sweepAngle: 360
            }
        }
    }

    Shape {
        id: arc
        anchors.fill: parent
        opacity: ring.arcSweep > 0 ? 1 : 0
        rotation: ring.spin
        transformOrigin: Item.Center
        layer.enabled: true
        layer.samples: 16
        layer.smooth: true
        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 120 : 0; easing.type: Theme.easeStandard } }
        ShapePath {
            fillColor: "transparent"
            strokeColor: ring.arcColor
            strokeWidth: ring.lineW
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: ring.width / 2; centerY: ring.height / 2
                radiusX: ring.ringSize / 2; radiusY: ring.ringSize / 2
                startAngle: -90
                sweepAngle: Math.max(0.01, ring.arcSweep)
            }
        }
    }

    Rectangle {
        id: dot
        anchors.centerIn: parent
        width: 14; height: 14; radius: 7
        color: ring.dotColor
        scale: ring.dotScale
        antialiasing: true
    }
}
