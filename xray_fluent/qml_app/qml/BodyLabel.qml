import QtQuick
import "."

// Fluent BodyLabel: standard body text bound to theme tokens.
Text {
    property bool strong: false
    color: Theme.text
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontNormal
    font.weight: strong ? Font.DemiBold : Font.Normal
    renderType: Text.QtRendering
    font.hintingPreference: Font.PreferVerticalHinting
    wrapMode: Text.WordWrap
    Behavior on color { ColorAnimation { duration: Theme.animFast } }
}
