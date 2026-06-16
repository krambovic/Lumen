import QtQuick
import "."

// Fluent CaptionLabel: small muted secondary text bound to theme tokens.
Text {
    color: Theme.textMuted
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSmall
    renderType: Text.NativeRendering
    wrapMode: Text.WordWrap
    Behavior on color { ColorAnimation { duration: Theme.animFast } }
}
