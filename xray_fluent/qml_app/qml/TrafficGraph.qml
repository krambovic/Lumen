import QtQuick
import App 1.0
import "."

// Real-time traffic sparkline
Item {
    id: root
    implicitHeight: 160

    readonly property int capacity: 60
    property var down: []
    property var up: []
    property real peak: 1

    readonly property color colorDown: "#3AA0FF"
    readonly property color colorUp:   "#27D17C"
    readonly property color colorGrid: Theme.dark ? Qt.rgba(1, 1, 1, 0.07) : Qt.rgba(0, 0, 0, 0.06)
    readonly property color colorBg:   Theme.dark ? Qt.rgba(1, 1, 1, 0.03) : Qt.rgba(0, 0, 0, 0.04)

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

            drawSeries(ctx, root.up,   root.colorUp,   w, h);
            drawSeries(ctx, root.down, root.colorDown, w, h);
        }

        // Build the list of points for a series mapped into the canvas rect.
        function points(data, w, h) {
            var n = data.length;
            var step = w / (root.capacity - 1);
            var x0 = w - (n - 1) * step;
            var pts = [];
            for (var i = 0; i < n; i++)
                pts.push({ x: x0 + i * step, y: h - (data[i] / root.peak) * (h - 6) - 3 });
            return pts;
        }

        // Trace a smooth path through pts using midpoint quadratic curves.
        function tracePath(ctx, pts) {
            ctx.moveTo(pts[0].x, pts[0].y);
            for (var k = 1; k < pts.length - 1; k++) {
                var mx = (pts[k].x + pts[k + 1].x) / 2;
                var my = (pts[k].y + pts[k + 1].y) / 2;
                ctx.quadraticCurveTo(pts[k].x, pts[k].y, mx, my);
            }
            ctx.lineTo(pts[pts.length - 1].x, pts[pts.length - 1].y);
        }

        function drawSeries(ctx, data, color, w, h) {
            var n = data.length;
            if (n < 2) return;
            var pts = points(data, w, h);
            var last = pts[n - 1];

            // gradient area fill under the curve
            var grad = ctx.createLinearGradient(0, 0, 0, h);
            grad.addColorStop(0, Qt.rgba(color.r, color.g, color.b, 0.34));
            grad.addColorStop(1, Qt.rgba(color.r, color.g, color.b, 0.02));
            ctx.beginPath();
            tracePath(ctx, pts);
            ctx.lineTo(last.x, h);
            ctx.lineTo(pts[0].x, h);
            ctx.closePath();
            ctx.fillStyle = grad;
            ctx.fill();

            // glowing smoothed line
            ctx.beginPath();
            tracePath(ctx, pts);
            ctx.strokeStyle = color;
            ctx.lineWidth = 2.4;
            ctx.lineJoin = "round";
            ctx.lineCap = "round";
            ctx.stroke();

            // leading dot
            ctx.beginPath();
            ctx.arc(last.x, last.y, 2.6, 0, Math.PI * 2);
            ctx.fillStyle = color;
            ctx.fill();
        }
    }

    Connections {
        target: App
        function onSettingsChanged() { canvas.requestPaint(); }
    }
}
