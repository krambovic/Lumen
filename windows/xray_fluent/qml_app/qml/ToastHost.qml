import QtQuick
import QtQuick.Layouts
import App 1.0
import "."

// Anchored bottom-right toast stack. Listens to App.toast(level, message) and
// shows a transient card that auto-dismisses. Replaces the old qfluentwidgets
// InfoBar pop-ups.
//
// NOTE: the bridge signal is toast(level, message) in that order, and the
// delegate reads the model roles explicitly (model property names, not the
// Text.text self-reference that previously left every toast blank).
Item {
    id: host
    anchors.fill: parent
    z: 1000

    function kindColor(kind) {
        if (kind === "error") return Theme.danger;
        if (kind === "warning") return Theme.warning;
        if (kind === "success") return Theme.success;
        return Theme.accent;
    }

    Connections {
        target: App
        function onToast(level, message) { host.show(message, level); }
        function onActionToast(level, message, actionId, actionLabel) {
            host.show(message, level, actionId, actionLabel);
        }
    }

    function show(message, kind, actionId, actionLabel) {
        if (!message || message.length === 0)
            return;
        toastModel.append({
            message: message,
            kind: kind || "info",
            actionId: actionId || "",
            actionLabel: actionLabel || ""
        });
    }

    ListModel { id: toastModel }

    ColumnLayout {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: 18
        anchors.bottomMargin: 18
        spacing: 8

        Repeater {
            model: toastModel
            delegate: Rectangle {
                id: toast
                required property string message
                required property string kind
                required property string actionId
                required property string actionLabel
                required property int index

                Layout.alignment: Qt.AlignRight
                radius: Theme.radius
                color: Theme.flyout
                border.width: 1
                border.color: Theme.flyoutBorder
                implicitWidth: Math.min(420, row.implicitWidth + 28)
                implicitHeight: row.implicitHeight + 20

                opacity: 0
                Component.onCompleted: appear.start()
                NumberAnimation { id: appear; target: toast; property: "opacity"; from: 0; to: 1; duration: 160 }

                Timer {
                    interval: 3200; running: true; repeat: false
                    onTriggered: disappear.start()
                }
                SequentialAnimation {
                    id: disappear
                    NumberAnimation { target: toast; property: "opacity"; to: 0; duration: 220 }
                    ScriptAction { script: toastModel.remove(toast.index) }
                }

                RowLayout {
                    id: row
                    anchors.centerIn: parent
                    width: parent.width - 28
                    spacing: 10
                    Rectangle {
                        width: 4; height: label.implicitHeight
                        radius: 2
                        color: host.kindColor(toast.kind)
                        Layout.alignment: Qt.AlignVCenter
                    }
                    Text {
                        id: label
                        text: I18n.t(toast.message)
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontNormal
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                    AccentButton {
                        visible: toast.actionId.length > 0
                        text: toast.actionLabel
                        kind: "accent"
                        onClicked: {
                            App.runToastAction(toast.actionId)
                            disappear.start()
                        }
                    }
                }
            }
        }
    }
}
