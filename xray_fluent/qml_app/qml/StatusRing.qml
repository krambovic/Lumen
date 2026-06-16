pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Effects
import App 1.0
import "."

// Connection status ring for the dashboard hero. Idle = faint stub, busy =
// spinning amber sweep, connected = full smooth green ring with a soft glow and
// a gently pulsing centre dot. Honours the Theme.animations master switch.
Item {
    id: ring
    property bool active: false   // connected
    property bool busy: false     // connecting / transition in progress
    implicitWidth: 60
    implicitHeight: 60

    readonly property color stateColor: busy ? Theme.warning : active ? Theme.success : Theme.textFaint
    readonly property real ringSize: Math.min(width, height) - 8
    readonly property real lineW: 4

    property real spin: 0
    NumberAnimation on spin {
        running: ring.busy && Theme.animations
        from: 0; to: 360; duration: 1100; loops: Animation.Infinite
    }
    onSpinChanged: if (ring.busy) sweep.requestPaint()
    onStateColorChanged: { sweep.requestPaint(); stub.requestPaint() }
    onBusyChanged: sweep.requestPaint()

    property real pulse: 1
    SequentialAnimation on pulse {
        running: ring.active && !ring.busy && Theme.animations
        loops: Animation.Infinite
        NumberAnimation { from: 1.0; to: 1.18; duration: 900; easing.type: Easing.InOutSine }
        NumberAnimation { from: 1.18; to: 1.0; duration: 900; easing.type: Easing.InOutSine }
    }

    // Background track — a perfectly round, GPU-antialiased ring (no canvas).
    Rectangle {
        anchors.centerIn: parent
        width: ring.ringSize; height: ring.ringSize; radius: width / 2
        color: "transparent"
        antialiasing: true
        border.width: ring.lineW
        border.color: Theme.dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0, 0, 0, 0.10)
    }

    // Connected: full coloured ring (smooth Rectangle, never aliased/square)
    // with an optional soft glow halo while animations are enabled.
    Rectangle {
        id: activeRing
        anchors.centerIn: parent
        visible: ring.active && !ring.busy
        width: ring.ringSize; height: ring.ringSize; radius: width / 2
        color: "transparent"
        antialiasing: true
        border.width: ring.lineW
        border.color: ring.stateColor
        layer.enabled: ring.active && !ring.busy && Theme.animations
        layer.effect: MultiEffect {
            shadowEnabled: true
            shadowColor: ring.stateColor
            shadowBlur: 0.55
            shadowOpacity: 0.6
            blurMax: 24
        }
    }

    // Busy: rotating partial sweep (a partial arc never reads as square).
    Canvas {
        id: sweep
        anchors.fill: parent
        visible: ring.busy
        antialiasing: true
        renderStrategy: Canvas.Threaded
        onPaint: {
            var ctx = getContext("2d");
            ctx.reset();
            var cx = width / 2, cy = height / 2;
            var r = ring.ringSize / 2;
            var start = ring.spin * Math.PI / 180;
            ctx.beginPath();
            ctx.arc(cx, cy, r, start, start + Math.PI * 0.6);
            ctx.lineWidth = ring.lineW;
            ctx.lineCap = "round";
            ctx.strokeStyle = ring.stateColor;
            ctx.stroke();
        }
    }

    // Idle: faint short stub near the top.
    Canvas {
        id: stub
        anchors.fill: parent
        visible: !ring.active && !ring.busy
        antialiasing: true
        renderStrategy: Canvas.Threaded
        onPaint: {
            var ctx = getContext("2d");
            ctx.reset();
            var cx = width / 2, cy = height / 2;
            var r = ring.ringSize / 2;
            ctx.beginPath();
            ctx.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 0.12);
            ctx.lineWidth = ring.lineW;
            ctx.lineCap = "round";
            ctx.strokeStyle = ring.stateColor;
            ctx.stroke();
        }
    }

    // Centre dot — pulses gently while connected.
    Rectangle {
        anchors.centerIn: parent
        width: 14; height: 14; radius: 7
        color: ring.stateColor
        scale: ring.pulse
        antialiasing: true
        Behavior on color { ColorAnimation { duration: Theme.animFast } }
    }

    Connections {
        target: App
        function onSettingsChanged() { sweep.requestPaint(); stub.requestPaint() }
    }
}
