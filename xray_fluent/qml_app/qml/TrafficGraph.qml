import QtQuick
import App 1.0
import "."

// Real-time traffic sparkline. Matches the original traffic_graph.py palette:
// download = blue (0,180,255), upload = green (0,220,120), faint grid + bg.
// Two ring buffers, repainted on metricsChanged via push().
Item {
    id: root
    implicitHeight: 160

    readonly property int capacity: 60
    property var down: []
    property var up: []
    property real peak: 1

    // colors identical to original widget build
    readonly property color colorDown: "#00B4FF"
    readonly property color colorUp:   "#00DC78"
    readonly property color colorGrid: Qt.rgba(1, 1, 1, 0.08)
    readonly property color colorBg:   Qt.rgba(0, 0, 0, 0.12)

    function push(d, u) {
        var nd = down.slice(); nd.push(d); if (nd.length > capacity) nd.shift();
        var nu = up.slice();   nu.push(u); if (nu.length > capacity) nu.shift();
        down = nd; up = nu;
        var m = 1;
        for (var i = 0; i < nd.length; i++) { if (nd[i] > m) m = nd[i]; }
        for (var j = 0; j < nu.length; j++) { if (nu[j] > m) m = nu[j]; }
        peak = m;
        canvas.requestPaint();
    }
    function reset() { down = []; up = []; peak = 1; canvas.requestPaint(); }

    Rectangle {
        anchors.fill: parent
        radius: Theme.radiusSmall
        color: root.colorBg
        border.width: 1
        border.color: Theme.borderSolid
    }

    Canvas {
        id: canvas
        anchors.fill: parent
        anchors.margins: 8
        renderStrategy: Canvas.Threaded

        onPaint: {
            var ctx = getContext("2d");
            ctx.reset();
            var w = width, h = height;
            ctx.clearRect(0, 0, w, h);

            // grid
            ctx.strokeStyle = root.colorGrid;
            ctx.lineWidth = 1;
            for (var g = 1; g < 4; g++) {
                var y = h * g / 4;
                ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(w, y); ctx.stroke();
            }

            drawSeries(ctx, root.down, root.colorDown, w, h);
            drawSeries(ctx, root.up,   root.colorUp,   w, h);
        }

        function drawSeries(ctx, data, color, w, h) {
            var n = data.length;
            if (n < 2) return;
            var step = w / (root.capacity - 1);
            var x0 = w - (n - 1) * step;
            ctx.beginPath();
            for (var i = 0; i < n; i++) {
                var x = x0 + i * step;
                var y = h - (data[i] / root.peak) * (h - 4) - 2;
                if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
            }
            ctx.strokeStyle = color;
            ctx.lineWidth = 2;
            ctx.lineJoin = "round";
            ctx.stroke();

            // soft fill under the line
            ctx.lineTo(x0 + (n - 1) * step, h);
            ctx.lineTo(x0, h);
            ctx.closePath();
            ctx.globalAlpha = 0.12;
            ctx.fillStyle = color;
            ctx.fill();
            ctx.globalAlpha = 1.0;
        }
    }

    Connections {
        target: App
        function onSettingsChanged() { canvas.requestPaint(); }
    }
}
