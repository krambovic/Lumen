import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

// Live port of ui/updates_page.py: two stacked cards — the application updater
// (current version, status, release notes, progress, Проверить / Скачать)
// and the Xray-core updater (version, status, Проверить / Обновить).
//
// Actions call App.checkAppUpdate / downloadAppUpdate / checkXrayUpdate /
// updateXrayCore; the bridge runs the existing background workers and reports
// progress back through the appUpdateState / xrayUpdateState signals, exactly
// mirroring MainWindow's update flow.
Item {
    id: page

    property string appVersion: App.appVersion
    property string xrayVersion: ""

    // application updater state
    property string appPhase: "idle"        // idle|checking|available|downloading|uptodate|ready|error
    property string appNotes: ""
    property string appMessage: ""
    property string appNewVersion: ""
    property int appPercent: 0

    // xray updater state
    property string xrayPhase: "idle"       // idle|checking|updating|available|updated|uptodate|error
    property string xrayMessage: ""

    readonly property bool appBusy: appPhase === "checking" || appPhase === "downloading"
    readonly property bool xrayBusy: xrayPhase === "checking" || xrayPhase === "updating"

    function appStatusText() {
        switch (appPhase) {
        case "checking":    return "Проверка обновлений..."
        case "uptodate":    return "У вас последняя версия"
        case "available":   return "Доступна новая версия: v" + appNewVersion
        case "downloading": return appMessage !== "" ? appMessage : ("Загрузка: " + appPercent + "%")
        case "ready":       return appMessage !== "" ? appMessage : "Обновление загружено. Перезапуск..."
        case "error":       return appMessage !== "" ? appMessage : "Ошибка проверки обновлений"
        default:            return "Нажмите «Проверить обновления», чтобы узнать о новых версиях."
        }
    }
    function appStatusColor() {
        if (appPhase === "error") return Theme.danger
        if (appPhase === "uptodate" || appPhase === "available" || appPhase === "ready") return Theme.success
        return Theme.textMuted
    }

    function xrayStatusText() {
        switch (xrayPhase) {
        case "checking": return "Проверка обновлений Xray..."
        case "updating": return "Обновление Xray..."
        case "uptodate":
        case "available":
        case "updated":
        case "error":    return xrayMessage
        default:         return "Проверьте наличие новой версии ядра Xray-core от разработчиков."
        }
    }
    function xrayStatusColor() {
        if (xrayPhase === "error") return Theme.danger
        if (xrayPhase === "updated" || xrayPhase === "uptodate" || xrayPhase === "available") return Theme.success
        return Theme.textMuted
    }

    function init() {
        var s = App.updatesInitialState()
        appVersion = s.appVersion || App.appVersion
        xrayVersion = s.xrayVersion || ""
    }
    Component.onCompleted: init()

    Connections {
        target: App
        function onAppUpdateState(s) {
            page.appPhase = s.phase || "idle"
            if (s.notes !== undefined) page.appNotes = s.notes
            if (s.version !== undefined) page.appNewVersion = s.version
            page.appMessage = s.message !== undefined ? s.message : ""
            if (s.percent !== undefined) page.appPercent = s.percent
            if (s.phase === "checking") { page.appNotes = ""; page.appPercent = 0 }
        }
        function onXrayUpdateState(s) {
            page.xrayPhase = s.phase || "idle"
            page.xrayMessage = s.message !== undefined ? s.message : ""
            if (s.version !== undefined && s.version !== "") page.xrayVersion = s.version
        }
    }

    FluentScroll {
        anchors.fill: parent

        ColumnLayout {
            width: parent.width
            spacing: Theme.spacingLarge

            Text {
                text: "Обновления"
                color: Theme.text; font.family: Theme.fontFamily
                font.pixelSize: Theme.fontTitle; font.weight: Font.DemiBold
            }

            // ---- application updater ----
            Card {
                Layout.fillWidth: true
                padding: 18
                hoverable: false
                ColumnLayout {
                    width: parent.width
                    spacing: 10
                    Text { text: "Приложение"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        Text { text: "Текущая версия:"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Text { text: "v" + page.appVersion; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; font.weight: Font.DemiBold }
                        Item { Layout.fillWidth: true }
                    }
                    Text {
                        text: page.appStatusText(); color: page.appStatusColor()
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        font.weight: (page.appPhase === "available" || page.appPhase === "uptodate" || page.appPhase === "ready" || page.appPhase === "error") ? Font.DemiBold : Font.Normal
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }

                    // release notes box (visible once we have notes)
                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 120
                        visible: page.appNotes !== ""
                        radius: Theme.radiusSmall
                        color: Theme.micaBase
                        border.width: 1; border.color: Theme.borderSolid
                        clip: true
                        Flickable {
                            anchors.fill: parent; anchors.margins: 10
                            contentHeight: notesText.implicitHeight
                            clip: true
                            Text {
                                id: notesText
                                width: parent.width
                                text: page.appNotes
                                color: Theme.textMuted; font.family: "Consolas"; font.pixelSize: 12; wrapMode: Text.WordWrap
                            }
                        }
                    }

                    // progress bar (checking = indeterminate, downloading = percent)
                    ProgressBar {
                        Layout.fillWidth: true
                        visible: page.appPhase === "checking" || page.appPhase === "downloading"
                        indeterminate: page.appPhase === "checking"
                        from: 0; to: 100; value: page.appPercent
                    }

                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        AccentButton {
                            kind: "accent"; glyph: "\uE72C"; text: "Проверить обновления"
                            enabled: !page.appBusy
                            onClicked: App.checkAppUpdate()
                        }
                        AccentButton {
                            kind: "ghost"; glyph: "\uE896"
                            text: page.appNewVersion !== "" ? ("Скачать v" + page.appNewVersion + " и установить") : "Скачать и установить"
                            visible: page.appPhase === "available" || page.appPhase === "downloading"
                            enabled: page.appPhase === "available"
                            onClicked: App.downloadAppUpdate()
                        }
                        Item { Layout.fillWidth: true }
                    }
                }
            }

            // ---- xray core updater ----
            Card {
                Layout.fillWidth: true
                padding: 18
                hoverable: false
                ColumnLayout {
                    width: parent.width
                    spacing: 10
                    Text { text: "Ядро Xray"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        Text { text: "Версия:"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Text { text: page.xrayVersion !== "" ? page.xrayVersion : "не найдена"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; font.weight: Font.DemiBold }
                        Item { Layout.fillWidth: true }
                    }
                    Text {
                        text: page.xrayStatusText(); color: page.xrayStatusColor()
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        font.weight: (page.xrayPhase === "updated" || page.xrayPhase === "error") ? Font.DemiBold : Font.Normal
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    ProgressBar {
                        Layout.fillWidth: true
                        visible: page.xrayBusy
                        indeterminate: true
                    }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        AccentButton {
                            kind: "accent"; glyph: "\uE72C"; text: "Проверить обновления Xray"
                            enabled: !page.xrayBusy
                            onClicked: App.checkXrayUpdate()
                        }
                        AccentButton {
                            kind: "ghost"; glyph: "\uE896"; text: "Обновить Xray core"
                            enabled: !page.xrayBusy
                            onClicked: App.updateXrayCore()
                        }
                        Item { Layout.fillWidth: true }
                    }
                }
            }

            Item { Layout.preferredHeight: 4 }
        }
    }
}
