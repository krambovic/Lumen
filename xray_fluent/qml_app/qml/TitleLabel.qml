import QtQuick
import "."

// Fluent SubtitleLabel/TitleLabel: a semibold heading bound to theme tokens.
Text {
    color: Theme.text
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontTitle
    font.weight: Font.DemiBold
    renderType: Text.NativeRendering
    wrapMode: Text.WordWrap
    Behavior on color { ColorAnimation { duration: Theme.animFast } }
}
