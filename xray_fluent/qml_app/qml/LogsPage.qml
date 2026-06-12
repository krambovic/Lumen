import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

Item {
    id: page

    property string filterText: ""

    // Scroll the log list to the newest entry. Defined as a named function so
    // it can be handed to Qt.callLater: passing logList.positionViewAtEnd
    // directly loses its `this` binding and silently does nothing (that is why
    // the previous "sticky bottom" never worked).
    function scrollLogsToBottom() {
        logList.positionViewAtEnd();
    }

    // The tab pages live in a StackLayout, which toggles `visible` on each
    // child. Whenever the Logs tab becomes visible (including the very first
    // time it is opened) re-pin to the bottom so the freshest lines show.
    onVisibleChanged: if (visible) {
        logList.stickToBottom = true;
        Qt.callLater(page.scrollLogsToBottom);
    }

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
            text: I18n.t("Логи и диагностика")
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
                placeholderText: I18n.t("Фильтр логов")
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
            AccentButton { text: I18n.t("Очистить"); kind: "ghost"; onClicked: App.clearLogs() }
            AccentButton {
                text: I18n.t("Экспорт диагностики")
                kind: "accent"
                onClicked: { if (typeof App.exportDiagnostics === "function") App.exportDiagnostics(); }
            }
        }

        Text {
            text: I18n.t("Логи работы")
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

                // Only auto-scroll to the newest line when the user is already
                // pinned to the bottom; otherwise keep their scroll position.
                property bool stickToBottom: true
                onMovementEnded: stickToBottom = atYEnd
                onCountChanged: if (stickToBottom) Qt.callLater(page.scrollLogsToBottom)
                // Open the tab already scrolled to the newest line.
                Component.onCompleted: Qt.callLater(page.scrollLogsToBottom)

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
                    text: I18n.t("Логи пусты")
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontNormal
                }
            }
        }
    }
}
