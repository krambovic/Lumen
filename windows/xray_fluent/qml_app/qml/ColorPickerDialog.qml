import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

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

    property bool showDarkness: false
    property bool showSystem: false
    property string heading: I18n.t("Цвет акцента")
    property string defaultColor: "#0078D4"

    readonly property var swatches: [
        "#0078D4", "#0099BC", "#00B294", "#107C10", "#498205", "#7A7574", "#515C6B", "#4C4A48",
        "#6B69D6", "#8764B8", "#744DA9", "#B146C2", "#E3008C", "#C30052", "#E81123", "#DA3B01",
        "#F7630C", "#FF8C00", "#FFB900", "#038387", "#567C73", "#847545", "#9D5D00", "#525E54"
    ]

    property real _h: 0
    property real _s: 0
    property real _v: 0
    property real _mute: 0
    readonly property color current: Qt.hsva(_h, _s * (1 - 0.80 * _mute), _v * (1 - 0.55 * _mute), 1)

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
        _mute = 0
    }
    function openWith(c, mute) { _loadHsv(c); _mute = mute === undefined ? 0 : mute; hexField.text = _hex(current); open() }
    function _baseHex() { return _hex(Qt.hsva(_h, _s, _v, 1)) }
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
            text: dlg.heading
            color: Theme.text; font.family: Theme.fontFamily
            font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
        }

        GridLayout {
            Layout.fillWidth: true
            columns: 8
            columnSpacing: 8
            rowSpacing: 8
            Repeater {
                model: dlg.swatches
                delegate: Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: width
                    radius: 6
                    color: modelData
                    readonly property bool sel: dlg._hex(dlg.current) === modelData.toUpperCase()
                    border.width: sel ? 2 : 1
                    border.color: sel ? Theme.text : Theme.divider
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { dlg._loadHsv(modelData); hexField.text = dlg._hex(dlg.current) }
                    }
                }
            }
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
                    width: 18; height: 18; radius: 9
                    color: "transparent"; border.width: 3; border.color: "#80000000"
                    x: dlg._s * sv.width - width / 2
                    y: (1 - dlg._v) * sv.height - height / 2
                    Rectangle {
                        anchors.centerIn: parent
                        width: 14; height: 14; radius: 7
                        color: "transparent"; border.width: 2; border.color: "white"
                    }
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
                    width: 8; height: parent.height + 8; radius: 4; y: -4
                    color: "white"; border.width: 1; border.color: "#80000000"
                    x: dlg._h * hue.width - width / 2
                }
                MouseArea {
                    anchors.fill: parent
                    onPressed: (m) => dlg._h = Math.max(0, Math.min(1, m.x / hue.width))
                    onPositionChanged: (m) => { if (pressed) dlg._h = Math.max(0, Math.min(1, m.x / hue.width)) }
                }
            }
        }

        Item {
            visible: dlg.showDarkness
            Layout.fillWidth: true
            Layout.preferredHeight: 16
            Rectangle {
                id: darkBar
                anchors.fill: parent
                radius: 4
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: Qt.hsva(dlg._h, dlg._s, dlg._v, 1) }
                    GradientStop { position: 1.0; color: Qt.hsva(dlg._h, dlg._s * 0.20, dlg._v * 0.45, 1) }
                }
                Rectangle {
                    width: 8; height: parent.height + 8; radius: 4; y: -4
                    color: "white"; border.width: 1; border.color: "#80000000"
                    x: dlg._mute * darkBar.width - width / 2
                }
                MouseArea {
                    anchors.fill: parent
                    function _set(mx) { dlg._mute = Math.max(0, Math.min(1, mx / darkBar.width)) }
                    onPressed: (m) => _set(m.x)
                    onPositionChanged: (m) => { if (pressed) _set(m.x) }
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
                FluentTextField {
                    id: hexField
                    Layout.fillWidth: true
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontStrong
                    selectByMouse: true
                    leftPadding: 10; rightPadding: 10
                    topPadding: 7; bottomPadding: 7
                    onEditingFinished: dlg._fromHex(text)
                    background: Rectangle {
                        radius: Theme.radius
                        color: Theme.dark ? Qt.rgba(1, 1, 1, 0.05) : Qt.rgba(0, 0, 0, 0.04)
                        border.width: 1
                        border.color: hexField.activeFocus ? Theme.accent : Theme.divider
                    }
                }
                Text {
                    text: "R " + Math.round(dlg.current.r * 255) + "    G " + Math.round(dlg.current.g * 255) + "    B " + Math.round(dlg.current.b * 255)
                    color: Theme.textFaint; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                }
            }
        }

        Flow {
            Layout.fillWidth: true
            spacing: 8
            visible: dlg.showSystem && Qt.platform.os === "windows"
            AccentButton {
                kind: "ghost"
                text: I18n.t("Системный")
                onClicked: { dlg._loadHsv(App.systemAccent()); hexField.text = dlg._hex(dlg.current) }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            AccentButton {
                kind: "ghost"
                text: I18n.t("Сбросить")
                onClicked: { dlg._loadHsv(dlg.defaultColor); hexField.text = dlg._hex(dlg.current) }
            }
            Item { Layout.fillWidth: true }
            AccentButton { kind: "ghost"; text: I18n.t("Отмена"); onClicked: dlg.close() }
            AccentButton { kind: "accent"; text: "OK"; onClicked: { dlg.accepted(dlg._hex(dlg.current)); dlg.close() } }
        }
    }
}
