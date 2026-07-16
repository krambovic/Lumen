import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import "."

// Shared WinUI-style content dialog.  Pages provide only title, content and
// accept/reject behavior; the surface, dimming and action row stay consistent.
Dialog {
    id: control

    property string okText: I18n.t("Добавить")
    property string cancelText: I18n.t("Отмена")
    property bool showOkButton: true
    property bool showCancelButton: true
    property bool okEnabled: true

    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    dim: true
    padding: 20
    topPadding: 12
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    background: Rectangle {
        radius: 10
        color: Theme.flyout
        border.width: 1
        border.color: Theme.flyoutBorder
    }

    Overlay.modal: Rectangle {
        color: Qt.rgba(0, 0, 0, 0.45)
    }

    header: Text {
        text: control.title
        visible: control.title.length > 0
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontStrong
        font.weight: Font.DemiBold
        wrapMode: Text.WordWrap
        leftPadding: 20
        rightPadding: 20
        topPadding: 18
        bottomPadding: 2
    }

    footer: RowLayout {
        visible: control.showCancelButton || control.showOkButton
        spacing: 10

        Item { Layout.fillWidth: true }

        AccentButton {
            visible: control.showCancelButton
            kind: "ghost"
            text: control.cancelText
            onClicked: control.reject()
            Layout.rightMargin: control.showOkButton ? 0 : 20
            Layout.topMargin: 6
            Layout.bottomMargin: 18
        }

        AccentButton {
            visible: control.showOkButton
            enabled: control.okEnabled
            kind: "accent"
            text: control.okText
            onClicked: control.accept()
            Layout.rightMargin: 20
            Layout.topMargin: 6
            Layout.bottomMargin: 18
        }
    }
}
