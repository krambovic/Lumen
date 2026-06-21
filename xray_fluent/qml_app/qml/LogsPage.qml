import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

Item {
    id: page

    property string activeLevel: "all"

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
                onTextChanged: App.setLogSearch(text)
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

        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            Repeater {
                model: [
                    { key: "all", title: I18n.t("Все") },
                    { key: "error", title: I18n.t("Ошибки") },
                    { key: "warning", title: I18n.t("Предупреждения") },
                    { key: "info", title: I18n.t("События") }
                ]
                delegate: AccentButton {
                    required property var modelData
                    kind: page.activeLevel === modelData.key ? "accent" : "ghost"
                    text: modelData.title
                    onClicked: {
                        page.activeLevel = modelData.key
                        App.setLogLevelFilter(modelData.key)
                    }
                }
            }
            Item { Layout.fillWidth: true }
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

                delegate: Rectangle {
                    width: logList.width
                    height: logContent.implicitHeight + 12
                    color: index % 2 === 0 ? "transparent" : Theme.card
                    radius: Theme.radiusSmall
                    Rectangle {
                        width: 3; height: parent.height - 8
                        anchors.left: parent.left
                        anchors.leftMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        radius: 2
                        color: page.levelColor(level)
                    }
                    ColumnLayout {
                        id: logContent
                        anchors.left: parent.left
                        anchors.leftMargin: 15
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            Text { text: time; color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            Text { text: source; color: page.levelColor(level); font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold }
                            Text {
                                Layout.fillWidth: true
                                text: line
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontNormal
                                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                            }
                            AccentButton {
                                visible: actionId.length > 0
                                text: I18n.t(actionLabel)
                                kind: "ghost"
                                onClicked: App.runToastAction(actionId)
                            }
                        }
                        Text {
                            visible: details.length > 0 && details !== line
                            Layout.fillWidth: true
                            text: details
                            color: Theme.textMuted
                            font.family: "Consolas, 'Courier New', monospace"
                            font.pixelSize: Theme.fontSmall
                            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                        }
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
