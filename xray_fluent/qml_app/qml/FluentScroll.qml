import QtQuick
import QtQuick.Controls
import "."

// Windows 11-style smooth vertical scroll container shared by every page.
// Replaces the chunky square Universal ScrollBar + default wheel handling:
//   * no bounce at the top / bottom edges (StopAtBounds)
//   * a gentle, immersive wheel: each notch drifts to a stop with a longer
//     softly-decelerating glide instead of snapping (tuned slower than the
//     first pass, closer to the original SmoothScrollArea feel)
//   * a thin, rounded Win11 scrollbar (FluentScrollBar) that stays faintly
//     visible while there is something to scroll and grows on hover
//
// Usage:  FluentScroll { ColumnLayout { width: parent.width; ... } }
Flickable {
    id: flick
    // Pixels travelled per wheel notch (before easing). Lower = slower wheel.
    property real wheelStep: 92
    // Glide duration per notch. Longer = softer, more immersive deceleration.
    property int glide: 540

    clip: true
    boundsBehavior: Flickable.StopAtBounds
    boundsMovement: Flickable.StopAtBounds
    contentWidth: width
    // Measure the reparented children INSIDE contentItem. Binding to the bare
    // childrenRect here resolves to contentItem.height (which is driven by
    // contentHeight) → a self-referential binding that stays stuck at the
    // initial value until a window resize perturbs it (the "won't scroll until
    // I resize" bug).
    contentHeight: contentItem.childrenRect.height
    flickDeceleration: 5000
    maximumFlickVelocity: 3000
    pixelAligned: true

    // Long, softly-decelerating glide for each wheel notch. OutQuint front-loads
    // the motion then eases out gently, so fast spins stay responsive while the
    // tail feels smooth and immersive rather than abrupt.
    NumberAnimation {
        id: scrollAnim
        target: flick; property: "contentY"
        duration: flick.glide; easing.type: Easing.OutQuint
    }

    WheelHandler {
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onWheel: (ev) => {
            var maxY = Math.max(0, flick.contentHeight - flick.height)
            if (maxY <= 0) { ev.accepted = true; return }
            if (ev.pixelDelta.y !== 0) {        // trackpad: pixel-precise, no glide
                scrollAnim.stop()
                flick.contentY = Math.max(0, Math.min(maxY, flick.contentY - ev.pixelDelta.y))
            } else {                            // mouse wheel: animated eased step
                var base = scrollAnim.running ? scrollAnim.to : flick.contentY
                var target = Math.max(0, Math.min(maxY, base - (ev.angleDelta.y / 120) * flick.wheelStep))
                scrollAnim.to = target
                scrollAnim.restart()
            }
            ev.accepted = true
        }
    }

    ScrollBar.vertical: FluentScrollBar {}
}
