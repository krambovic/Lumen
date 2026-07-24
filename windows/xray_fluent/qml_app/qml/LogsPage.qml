import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

Item {
    id: page

    property string activeLevel: "all"
    property bool autoScrollEnabled: true

    Timer {
        id: searchDebounce
        interval: 160
        repeat: false
        onTriggered: App.setLogSearch(searchField.text)
    }

    function scrollLogsToBottom() {
        logList.positionViewAtEnd();
    }

    function setAutoScrollEnabled(enabled) {
        autoScrollEnabled = enabled;
        if (enabled)
            Qt.callLater(page.scrollLogsToBottom);
    }

    onVisibleChanged: if (visible && autoScrollEnabled)
        Qt.callLater(page.scrollLogsToBottom);

    function levelColor(level) {
        if (level === "error") return Theme.danger;
        if (level === "warning") return Theme.warning;
        if (level === "success") return Theme.success;
        return Theme.textMuted;
    }

    function levelLabel(level) {
        if (level === "error") return "ERROR";
        if (level === "warning") return "WARNING";
        if (level === "success") return "EVENT";
        return "INFO";
    }

    function messageWithoutLevel(value) {
        return String(value || "").replace(/^(?:ERROR|WARNING|WARN|INFO|DEBUG|FATAL|PANIC|CRITICAL)\b[\s:\-]*/i, "");
    }

    function htmlEscape(value) {
        return String(value || "")
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/\n/g, "<br>");
    }

    function formattedLog(time, source, level, line, details) {
        var prefix = htmlEscape(time + "  " + source + "  " + levelLabel(level));
        var body = messageWithoutLevel(line);
        if (details.length > 0 && details !== line)
            body += "\n" + messageWithoutLevel(details);
        return "<font color=\"" + String(levelColor(level)) + "\">" + prefix
            + "</font>&nbsp;&nbsp;" + htmlEscape(body);
    }

    component SelectableLogText: FluentTextEdit {
        readOnly: true
        selectByMouse: true
        persistentSelection: false
        cursorVisible: false
        activeFocusOnTab: false
        color: Theme.text
        selectionColor: Theme.accentSoft
        selectedTextColor: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontNormal
        wrapMode: TextEdit.WrapAtWordBoundaryOrAnywhere
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

            FluentTextField {
                id: searchField
                Layout.fillWidth: true
                placeholderText: I18n.t("Фильтр логов")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                selectByMouse: true
                onTextChanged: searchDebounce.restart()
                background: Rectangle {
                    radius: Theme.radiusSmall
                    color: Theme.card
                    border.width: 1
                    border.color: searchField.activeFocus ? Theme.accent : Theme.borderSolid
                }
            }
            AccentButton {
                text: I18n.t("Очистить")
                kind: "ghost"
                onClicked: App.clearLogs()
            }
            AccentButton {
                text: I18n.t("Экспорт диагностики")
                kind: "accent"
                onClicked: exportDialog.open()
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
                    { key: "success", title: I18n.t("События") },
                    { key: "info", title: I18n.t("Работа") }
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
            Text {
                text: I18n.t("Автопрокрутка")
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                color: Theme.textMuted
            }
            Switch {
                id: autoScrollSwitch
                checked: page.autoScrollEnabled
                onToggled: page.setAutoScrollEnabled(checked)
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            hoverable: false

            ListView {
                id: logList
                anchors.fill: parent
                clip: true
                model: App.logModel
                spacing: 1
                cacheBuffer: 800
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: FluentScrollBar {}

                onCountChanged: {
                    if (page.autoScrollEnabled)
                        Qt.callLater(page.scrollLogsToBottom);
                }
                Component.onCompleted: Qt.callLater(page.scrollLogsToBottom)

                delegate: Rectangle {
                    width: logList.width
                    height: logContent.implicitHeight + 12
                    color: index % 2 === 0 ? "transparent" : Theme.card
                    radius: Theme.radiusSmall

                    Rectangle {
                        width: 3
                        height: parent.height - 8
                        anchors.left: parent.left
                        anchors.leftMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        radius: 2
                        color: page.levelColor(level)
                    }

                    RowLayout {
                        id: logContent
                        anchors.left: parent.left
                        anchors.leftMargin: 15
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8

                        SelectableLogText {
                            Layout.fillWidth: true
                            textFormat: TextEdit.RichText
                            text: page.formattedLog(time, source, level, line, details)
                        }

                        AccentButton {
                            visible: actionId.length > 0
                            text: I18n.t(actionLabel)
                            kind: "ghost"
                            onClicked: App.runToastAction(actionId)
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

    DiagnosticsExportDialog { id: exportDialog }
}
