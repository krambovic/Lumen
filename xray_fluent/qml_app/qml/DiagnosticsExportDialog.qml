import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import App 1.0
import "."

Popup {
    id: dlg
    modal: true
    dim: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    parent: Overlay.overlay
    anchors.centerIn: Overlay.overlay
    width: 540
    height: Math.min(col.implicitHeight, (Overlay.overlay ? Overlay.overlay.height : 720) - 48)

    background: Rectangle {
        color: Theme.flyout
        radius: 10
        border.width: 1
        border.color: Theme.flyoutBorder
    }

    Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.4) }

    // ── reusable section row: title + subtitle + switch ───────────────
    component OptRow: RowLayout {
        id: row
        Layout.fillWidth: true
        spacing: 12
        property string title: ""
        property string subtitle: ""
        property bool defaultChecked: true
        property alias checked: sw.checked

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 1
            Text {
                Layout.fillWidth: true
                text: row.title
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
            }
            Text {
                Layout.fillWidth: true
                visible: row.subtitle.length > 0
                text: row.subtitle
                color: Theme.textMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap
            }
        }
        Switch { id: sw; checked: row.defaultChecked }
    }

    contentItem: ColumnLayout {
        id: col
        spacing: 12

        Text {
            Layout.fillWidth: true
            Layout.margins: 20
            Layout.bottomMargin: 0
            text: I18n.t("Экспорт диагностики")
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontTitle
            font.bold: true
        }

        Text {
            Layout.fillWidth: true
            Layout.leftMargin: 20
            Layout.rightMargin: 20
            text: I18n.t("Выберите, что включить в архив диагностики.")
            color: Theme.textMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            wrapMode: Text.WordWrap
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 20
            Layout.rightMargin: 20
            spacing: 12

            OptRow {
                id: rowErrors
                title: I18n.t("Ошибки")
                subtitle: I18n.t("Файл errors.log — предупреждения и ошибки")
            }
            OptRow {
                id: rowCore
                title: I18n.t("Логи ядра")
                subtitle: I18n.t("Файл core.log — xray / sing-box / zapret / TUN")
            }
            OptRow {
                id: rowApp
                title: I18n.t("Логи приложения")
                subtitle: I18n.t("Файл app.log — события интерфейса и логики")
            }
            OptRow {
                id: rowTraffic
                title: I18n.t("Логи трафика")
                subtitle: I18n.t("Файл traffic.log — скорость, пинг, авто-переключение")
            }
            OptRow {
                id: rowState
                title: I18n.t("Состояние приложения")
                subtitle: I18n.t("Настройки и список серверов (секреты вырезаны)")
            }
            OptRow {
                id: rowRecent
                title: I18n.t("Недавние логи интерфейса")
                subtitle: I18n.t("Последние строки из текущей сессии")
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.leftMargin: 20
            Layout.rightMargin: 20
            height: 1
            color: Theme.borderSolid
        }

        OptRow {
            id: rowUpload
            Layout.leftMargin: 20
            Layout.rightMargin: 20
            enabled: App.diagnosticsUploadEnabled
            defaultChecked: App.diagnosticsUploadEnabled
            title: I18n.t("Отправить на сервер")
            subtitle: I18n.t("Автоматически загрузить архив на сервер диагностики")
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
                text: I18n.t("Экспортировать")
                onClicked: {
                    App.runDiagnosticsExport({
                        "errors": rowErrors.checked,
                        "core": rowCore.checked,
                        "app": rowApp.checked,
                        "traffic": rowTraffic.checked,
                        "state": rowState.checked,
                        "recent": rowRecent.checked,
                        "upload": App.diagnosticsUploadEnabled && rowUpload.checked
                    });
                    dlg.close();
                }
            }
        }
    }
}
