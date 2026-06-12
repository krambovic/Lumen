import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import "."

// Fluent-style accent colour picker rendered as a modal overlay INSIDE the
// app window (reparented to the shared Overlay), mirroring qfluentwidgets'
// ColorDialog: an HSV saturation/value square + hue slider + hex/RGB readout.
// Open it with openWith(currentColor); it emits accepted(hex) on confirm.
Popup {
    id: dlg
    modal: true
    dim: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    padding: 20

    parent: Overlay.overlay
    anchors.centerIn: Overlay.overlay
    width: 340

    signal accepted(string hex)

    // Default system accent, used by the I18n.t("Сбросить") button to restore the
    // out-of-the-box colour (matches Theme.accent's default of #0078D4).
    property string defaultColor: "#0078D4"

    // HSV working state (0..1), kept separate so dragging stays smooth.
    property real _h: 0
    property real _s: 0
    property real _v: 0
    readonly property color current: Qt.hsva(_h, _s, _v, 1)

    function _pad(v) { return ("0" + Math.round(v * 255).toString(16)).slice(-2) }
    function _hex(c) { return ("#" + _pad(c.r) + _pad(c.g) + _pad(c.b)).toUpperCase() }
    function _loadHsv(c) {
        var col = (typeof c === "string") ? Qt.color(c) : c
        _h = col.hsvHue < 0 ? _h : col.hsvHue
        _s = col.hsvSaturation
        _v = col.hsvValue
    }
    function _fromHex(s) {
        var t = s.replace("#", "")
        if (!/^[0-9A-Fa-f]{6}$/.test(t)) return
        _loadHsv(Qt.rgba(parseInt(t.substr(0, 2), 16) / 255,
                         parseInt(t.substr(2, 2), 16) / 255,
                         parseInt(t.substr(4, 2), 16) / 255, 1))
    }
    function openWith(c) { _loadHsv(c); hexField.text = _hex(current); open() }
    onCurrentChanged: if (!hexField.activeFocus) hexField.text = _hex(current)

    background: Rectangle {
        radius: Theme.radius
        color: Theme.flyout
        border.width: 1
        border.color: Theme.flyoutBorder
    }

    contentItem: ColumnLayout {
        spacing: 14

        Text {
            text: I18n.t("Цвет акцента")
            color: Theme.text; font.family: Theme.fontFamily
            font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
        }

        // saturation / value square
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: 180
            Rectangle {
                id: sv
                anchors.fill: parent
                radius: 6
                color: Qt.hsva(dlg._h, 1, 1, 1)
                Rectangle {
                    anchors.fill: parent; radius: 6
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0.0; color: "#ffffffff" }
                        GradientStop { position: 1.0; color: "#00ffffff" }
                    }
                }
                Rectangle {
                    anchors.fill: parent; radius: 6
                    gradient: Gradient {
                        orientation: Gradient.Vertical
                        GradientStop { position: 0.0; color: "#00000000" }
                        GradientStop { position: 1.0; color: "#ff000000" }
                    }
                }
                Rectangle {
                    width: 16; height: 16; radius: 8
                    color: "transparent"; border.width: 2; border.color: "white"
                    x: dlg._s * sv.width - width / 2
                    y: (1 - dlg._v) * sv.height - height / 2
                }
                MouseArea {
                    anchors.fill: parent
                    onPressed: (m) => { dlg._s = Math.max(0, Math.min(1, m.x / sv.width)); dlg._v = Math.max(0, Math.min(1, 1 - m.y / sv.height)) }
                    onPositionChanged: (m) => { if (pressed) { dlg._s = Math.max(0, Math.min(1, m.x / sv.width)); dlg._v = Math.max(0, Math.min(1, 1 - m.y / sv.height)) } }
                }
            }
        }

        // hue slider
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: 16
            Rectangle {
                id: hue
                anchors.fill: parent
                radius: 4
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.000; color: "#ff0000" }
                    GradientStop { position: 0.167; color: "#ffff00" }
                    GradientStop { position: 0.333; color: "#00ff00" }
                    GradientStop { position: 0.500; color: "#00ffff" }
                    GradientStop { position: 0.667; color: "#0000ff" }
                    GradientStop { position: 0.833; color: "#ff00ff" }
                    GradientStop { position: 1.000; color: "#ff0000" }
                }
                Rectangle {
                    width: 6; height: parent.height + 6; radius: 3; y: -3
                    color: "transparent"; border.width: 2; border.color: "white"
                    x: dlg._h * hue.width - width / 2
                }
                MouseArea {
                    anchors.fill: parent
                    onPressed: (m) => dlg._h = Math.max(0, Math.min(1, m.x / hue.width))
                    onPositionChanged: (m) => { if (pressed) dlg._h = Math.max(0, Math.min(1, m.x / hue.width)) }
                }
            }
        }

        // preview + hex + rgb
        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            Rectangle {
                width: 46; height: 46; radius: 6
                color: dlg.current
                border.width: 1; border.color: Theme.divider
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6
                TextField {
                    id: hexField
                    Layout.fillWidth: true
                    onEditingFinished: dlg._fromHex(text)
                }
                Text {
                    text: "R " + Math.round(dlg.current.r * 255) + "    G " + Math.round(dlg.current.g * 255) + "    B " + Math.round(dlg.current.b * 255)
                    color: Theme.textFaint; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                }
            }
        }

        // actions
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            AccentButton {
                kind: "ghost"
                text: I18n.t("Сбросить")
                // Preview the default accent in the picker; user confirms with OK.
                onClicked: { dlg._loadHsv(dlg.defaultColor); hexField.text = dlg._hex(dlg.current) }
            }
            Item { Layout.fillWidth: true }
            AccentButton { kind: "ghost"; text: I18n.t("Отмена"); onClicked: dlg.close() }
            AccentButton { kind: "accent"; text: "OK"; onClicked: { dlg.accepted(dlg._hex(dlg.current)); dlg.close() } }
        }
    }
}
