import QtQuick
import QtQuick.Layouts
import "."

// Generic section scaffold used by pages whose backend bridge is still being
// wired (Configs editor, Zapret, History charts, Updates). The navigation,
// layout and theming are final; only the inner controls are pending.
Item {
    id: page
    property string title: ""
    property string glyph: "\uE9CE"
    property string iconFont: "Segoe Fluent Icons"
    property string note: "Этот раздел в разработке."

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacingLarge

        Text {
            text: page.title
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontTitle
            font.weight: Font.DemiBold
            color: Theme.text
        }

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            ColumnLayout {
                anchors.centerIn: parent
                width: Math.min(parent.width - 40, 420)
                spacing: 14
                Text {
                    text: page.glyph
                    Layout.alignment: Qt.AlignHCenter
                    font.family: page.iconFont
                    font.pixelSize: 44
                    color: Theme.textFaint
                }
                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    text: page.note
                    color: Theme.textMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontNormal
                }
            }
        }
    }
}
