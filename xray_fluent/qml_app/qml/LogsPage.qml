import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

Item {
    id: page

    property string filterText: ""

    function levelColor(level) {
        if (level === "error") return Theme.danger;
        if (level === "warning") return Theme.warning;
        if (level === "success") return Theme.success;
        return Theme.textMuted;
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacing

        Text {
            text: "Логи и диагностика"
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontTitle
            font.weight: Font.Bold
            color: Theme.text
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            TextField {
                id: searchField
                Layout.fillWidth: true
                placeholderText: "Фильтр логов"
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                selectByMouse: true
                onTextChanged: page.filterText = text
                background: Rectangle {
                    radius: Theme.radiusSmall
                    color: Theme.card
                    border.width: 1
                    border.color: searchField.activeFocus ? Theme.accent : Theme.borderSolid
                }
            }
            AccentButton { text: "Очистить"; kind: "ghost"; onClicked: App.clearLogs() }
            AccentButton {
                text: "Экспорт диагностики"
                kind: "accent"
                onClicked: { if (typeof App.exportDiagnostics === "function") App.exportDiagnostics(); }
            }
        }

        Text {
            text: "Логи работы"
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontNormal
            color: Theme.textMuted
        }

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            ListView {
                id: logList
                anchors.fill: parent
                clip: true
                model: App.logModel
                spacing: 1
                cacheBuffer: 800
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: FluentScrollBar {}

                onCountChanged: positionViewAtEnd()

                delegate: Row {
                    width: logList.width
                    spacing: 8
                    visible: page.filterText === "" || line.toLowerCase().indexOf(page.filterText.toLowerCase()) >= 0
                    height: visible ? lineText.implicitHeight : 0
                    Rectangle {
                        width: 3; height: lineText.implicitHeight
                        color: page.levelColor(level)
                    }
                    Text {
                        id: lineText
                        width: parent.width - 11
                        text: line
                        color: Theme.textMuted
                        font.family: "Consolas, 'Courier New', monospace"
                        font.pixelSize: Theme.fontSmall
                        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                    }
                }

                Text {
                    anchors.centerIn: parent
                    visible: logList.count === 0
                    text: "Логи пусты"
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontNormal
                }
            }
        }
    }
}
