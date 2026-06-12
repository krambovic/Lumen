import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import App 1.0
import "."

// «Массовое редактирование» — QML twin of ui/bulk_edit_dialog.py.
//
// Collects an optional target group plus tag add/remove lists and applies them
// to the selected node ids via App.bulkEditNodes. Empty fields are skipped
// (same semantics as the classic dialog).
Popup {
    id: dlg
    modal: true
    dim: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape

    parent: Overlay.overlay
    anchors.centerIn: Overlay.overlay
    width: 440

    property var ids: []
    property int count: 0

    background: Rectangle {
        color: Theme.flyout
        radius: 10
        border.width: 1
        border.color: Theme.flyoutBorder
    }

    Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.4) }

    function openFor(nodeIds) {
        ids = nodeIds || []
        count = ids.length
        groupField.text = ""
        addField.text = ""
        removeField.text = ""
        open()
    }

    function _split(s) {
        var out = []
        var parts = (s || "").split(",")
        for (var i = 0; i < parts.length; i++) {
            var t = parts[i].trim()
            if (t) out.push(t)
        }
        return out
    }

    function _apply() {
        App.bulkEditNodes(ids, {
            "group": groupField.text,
            "add_tags": _split(addField.text),
            "remove_tags": _split(removeField.text)
        })
        close()
    }

    component FLabel: Text {
        Layout.fillWidth: true
        color: Theme.textMuted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        wrapMode: Text.WordWrap
    }
    component FField: TextField {
        Layout.fillWidth: true
        implicitHeight: Theme.controlHeight
        leftPadding: 10
        rightPadding: 10
        topPadding: 0
        bottomPadding: 0
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontNormal
        selectByMouse: true
        verticalAlignment: TextInput.AlignVCenter
        background: Rectangle {
            radius: Theme.radiusSmall
            color: Theme.controlFill
            border.width: 1
            border.color: parent.activeFocus ? Theme.accent : Theme.borderSolid
        }
    }

    contentItem: ColumnLayout {
        spacing: 10

        Text {
            Layout.fillWidth: true
            Layout.topMargin: 20
            Layout.leftMargin: 20
            Layout.rightMargin: 20
            text: I18n.t("Массовое редактирование")
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontTitle
            font.bold: true
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 20
            Layout.rightMargin: 20
            spacing: 6

            Text {
                Layout.fillWidth: true
                text: I18n.t("Выбрано нод: ") + dlg.count
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
            }

            FLabel { text: I18n.t("Переместить в группу (пусто = пропустить)") }
            FField { id: groupField }

            FLabel { text: I18n.t("Добавить теги (через запятую, пусто = пропустить)") }
            FField { id: addField; placeholderText: I18n.t("тег1, тег2") }

            FLabel { text: I18n.t("Удалить теги (через запятую, пусто = пропустить)") }
            FField { id: removeField; placeholderText: I18n.t("тег1, тег2") }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 20
            Layout.topMargin: 6
            spacing: 10
            Item { Layout.fillWidth: true }
            AccentButton {
                kind: "ghost"
                text: I18n.t("Отмена")
                onClicked: dlg.close()
            }
            AccentButton {
                kind: "accent"
                text: I18n.t("Применить")
                onClicked: dlg._apply()
            }
        }
    }
}
