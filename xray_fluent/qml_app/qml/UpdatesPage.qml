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
    property string singboxVersion: ""
    property string drouteVersion: ""

    // application updater state
    property string appPhase: "idle"        // idle|checking|available|downloading|uptodate|ready|error
    property string appNotes: ""
    property string appMessage: ""
    property string appNewVersion: ""
    property int appPercent: 0
    property string appChannel: "stable"   // stable|prerelease
    property bool appIsDowngrade: false     // true — целевая версия ниже (переход/откат)

    // xray updater state
    property string xrayPhase: "idle"       // idle|checking|updating|available|updated|uptodate|error
    property string xrayMessage: ""
    property int xrayPercent: 0
    property string singboxPhase: "idle"
    property string singboxMessage: ""
    property int singboxPercent: 0
    property string geodataPhase: "idle"
    property string geodataMessage: ""
    property int geodataPercent: 0
    property string droutePhase: "idle"
    property string drouteMessage: ""
    property int droutePercent: 0

    readonly property bool appBusy: appPhase === "checking" || appPhase === "downloading"
    readonly property bool xrayBusy: xrayPhase === "checking" || xrayPhase === "updating"
    readonly property bool singboxBusy: singboxPhase === "checking" || singboxPhase === "updating"
    readonly property bool geodataBusy: geodataPhase === "checking" || geodataPhase === "updating"
    readonly property bool drouteBusy: droutePhase === "checking" || droutePhase === "updating"

    function appStatusText() {
        switch (appPhase) {
        case "checking":    return I18n.t("Проверка обновлений...")
        case "uptodate":    return I18n.t("У вас последняя версия")
        case "available":   return (page.appIsDowngrade ? I18n.t("Доступен переход на v") : I18n.t("Доступна новая версия: v")) + appNewVersion
        case "downloading": return appMessage !== "" ? I18n.t(appMessage) : (I18n.t("Загрузка: ") + appPercent + "%")
        case "ready":       return appMessage !== "" ? I18n.t(appMessage) : I18n.t("Обновление загружено. Перезапуск...")
        case "error":       return appMessage !== "" ? I18n.t(appMessage) : I18n.t("Ошибка проверки обновлений")
        default:            return I18n.t("Нажмите «Проверить обновления», чтобы узнать о новых версиях.")
        }
    }
    function appStatusColor() {
        if (appPhase === "error") return Theme.danger
        if (appPhase === "uptodate" || appPhase === "available" || appPhase === "ready") return Theme.success
        return Theme.textMuted
    }

    function xrayStatusText() {
        switch (xrayPhase) {
        case "checking": return I18n.t("Проверка обновлений Xray...")
        case "updating": return xrayPercent > 0 ? (I18n.t("Обновление Xray...") + " " + xrayPercent + "%") : I18n.t("Обновление Xray...")
        case "uptodate":
        case "available":
        case "updated":
        case "error":    return I18n.t(xrayMessage)
        default:         return I18n.t("Проверьте наличие новой версии ядра Xray-core от разработчиков.")
        }
    }
    function xrayStatusColor() {
        if (xrayPhase === "error") return Theme.danger
        if (xrayPhase === "updated" || xrayPhase === "uptodate" || xrayPhase === "available") return Theme.success
        return Theme.textMuted
    }
    function resourceStatusText(phase, message, fallback, percent) {
        switch (phase) {
        case "checking": return I18n.t("Проверка обновлений...")
        case "updating": return percent > 0 ? (I18n.t("Обновление...") + " " + percent + "%") : I18n.t("Обновление...")
        case "uptodate":
        case "available":
        case "updated":
        case "error":    return I18n.t(message)
        default:         return fallback
        }
    }
    function resourceStatusColor(phase) {
        if (phase === "error") return Theme.danger
        if (phase === "updated" || phase === "uptodate" || phase === "available") return Theme.success
        return Theme.textMuted
    }
    function escapeHtml(value) {
        return String(value || "")
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
    }
    function formatMarkdownInline(value) {
        var text = escapeHtml(value)
        text = text.replace(/`([^`]+)`/g, "<span style=\"font-family:'Consolas'; color:" + Theme.text + ";\">$1</span>")
        text = text.replace(/\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g, "<a href=\"$2\" style=\"color:" + Theme.accent + "; text-decoration:none;\">$1</a>")
        text = text.replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
        text = text.replace(/(^|[^*])\*([^*]+)\*/g, "$1<span style=\"font-style:italic; text-decoration:none;\">$2</span>")
        text = text.replace(/(^|[^_])_([^_]+)_/g, "$1<span style=\"font-style:italic; text-decoration:none;\">$2</span>")
        return text
    }
    function formatReleaseNotes(value) {
        var lines = String(value || "").replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n")
        var out = []
        for (var i = 0; i < lines.length; ++i) {
            var line = lines[i]
            if (line.trim() === "") {
                out.push("<br>")
            } else if (line.indexOf("## ") === 0) {
                out.push("<div style=\"font-size:14px; font-weight:600; color:" + Theme.text + "; margin-bottom:4px;\">" + formatMarkdownInline(line.slice(3)) + "</div>")
            } else if (line.indexOf("# ") === 0) {
                out.push("<div style=\"font-size:15px; font-weight:600; color:" + Theme.text + "; margin-bottom:4px;\">" + formatMarkdownInline(line.slice(2)) + "</div>")
            } else if (line.indexOf("- ") === 0) {
                out.push("<div style=\"margin-left:2px;\">• " + formatMarkdownInline(line.slice(2)) + "</div>")
            } else {
                out.push("<div>" + formatMarkdownInline(line) + "</div>")
            }
        }
        return out.join("")
    }

    function init() {
        var s = App.updatesInitialState()
        appVersion = s.appVersion || App.appVersion
        xrayVersion = s.xrayVersion || ""
        singboxVersion = s.singboxVersion || ""
        drouteVersion = s.drouteVersion || ""
        appChannel = App.releaseChannel
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
            if (s.channel !== undefined) page.appChannel = s.channel
            page.appIsDowngrade = (s.isDowngrade === true)
            if (s.phase === "checking") { page.appNotes = ""; page.appPercent = 0 }
        }
        function onXrayUpdateState(s) {
            page.xrayPhase = s.phase || "idle"
            page.xrayMessage = s.message !== undefined ? s.message : ""
            if (s.percent !== undefined) page.xrayPercent = s.percent
            if (s.phase === "checking") page.xrayPercent = 0
            if (s.version !== undefined && s.version !== "") page.xrayVersion = s.version
        }
        function onResourceUpdateState(s) {
            if ((s.kind || "") === "singbox") {
                page.singboxPhase = s.phase || "idle"
                page.singboxMessage = s.message !== undefined ? s.message : ""
                if (s.percent !== undefined) page.singboxPercent = s.percent
                if (s.phase === "checking") page.singboxPercent = 0
                if (s.version !== undefined && s.version !== "") page.singboxVersion = s.version
            } else if ((s.kind || "") === "geodata") {
                page.geodataPhase = s.phase || "idle"
                page.geodataMessage = s.message !== undefined ? s.message : ""
                if (s.percent !== undefined) page.geodataPercent = s.percent
            } else if ((s.kind || "") === "droute") {
                page.droutePhase = s.phase || "idle"
                page.drouteMessage = s.message !== undefined ? s.message : ""
                if (s.percent !== undefined) page.droutePercent = s.percent
                if (s.phase === "checking") page.droutePercent = 0
                if (s.version !== undefined && s.version !== "") page.drouteVersion = s.version
            }
        }
    }

    FluentScroll {
        anchors.fill: parent
        roundedClip: false

        ColumnLayout {
            width: parent.width
            spacing: Theme.spacingLarge

            Text {
                text: I18n.t("Обновления")
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
                    Text { text: I18n.t("Приложение"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        Text { text: I18n.t("Текущая версия:"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Text { text: "v" + page.appVersion; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; font.weight: Font.DemiBold }
                        Item { Layout.fillWidth: true }
                        // Сегментированный переключатель канала обновлений приложения
                        // (на ядра Xray/sing-box не влияет).
                        Text { text: I18n.t("Канал:"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Rectangle {
                            implicitHeight: 30
                            implicitWidth: segRow.implicitWidth + 4
                            radius: height / 2
                            color: Theme.controlFill
                            border.width: 1
                            border.color: Theme.borderSolid
                            opacity: page.appBusy ? 0.5 : 1.0

                            Row {
                                id: segRow
                                anchors.centerIn: parent
                                spacing: 0

                                Item {
                                    width: Math.max(stableTxt.implicitWidth + 26, 76); height: 26
                                    Rectangle {
                                        anchors.fill: parent; radius: height / 2
                                        color: page.appChannel === "stable" ? Theme.accent
                                             : (stableMouse.containsMouse ? Theme.controlFillHover : "transparent")
                                        Behavior on color { ColorAnimation { duration: 120 } }
                                    }
                                    Text {
                                        id: stableTxt; anchors.centerIn: parent; text: I18n.t("Stable")
                                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                                        font.weight: page.appChannel === "stable" ? Font.DemiBold : Font.Normal
                                        color: page.appChannel === "stable" ? Theme.accentText : Theme.textMuted
                                    }
                                    MouseArea {
                                        id: stableMouse; anchors.fill: parent; hoverEnabled: true
                                        enabled: !page.appBusy
                                        cursorShape: page.appBusy ? Qt.ArrowCursor : Qt.PointingHandCursor
                                        onClicked: if (page.appChannel !== "stable") { page.appChannel = "stable"; App.setReleaseChannel("stable") }
                                    }
                                }
                                Item {
                                    width: Math.max(preTxt.implicitWidth + 26, 76); height: 26
                                    Rectangle {
                                        anchors.fill: parent; radius: height / 2
                                        color: page.appChannel === "prerelease" ? Theme.accent
                                             : (preMouse.containsMouse ? Theme.controlFillHover : "transparent")
                                        Behavior on color { ColorAnimation { duration: 120 } }
                                    }
                                    Text {
                                        id: preTxt; anchors.centerIn: parent; text: I18n.t("Pre-release")
                                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                                        font.weight: page.appChannel === "prerelease" ? Font.DemiBold : Font.Normal
                                        color: page.appChannel === "prerelease" ? Theme.accentText : Theme.textMuted
                                    }
                                    MouseArea {
                                        id: preMouse; anchors.fill: parent; hoverEnabled: true
                                        enabled: !page.appBusy
                                        cursorShape: page.appBusy ? Qt.ArrowCursor : Qt.PointingHandCursor
                                        onClicked: if (page.appChannel !== "prerelease") { page.appChannel = "prerelease"; App.setReleaseChannel("prerelease") }
                                    }
                                }
                            }
                        }
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
                            id: notesFlickable
                            anchors.fill: parent; anchors.margins: 10
                            contentHeight: notesText.implicitHeight
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            WheelHandler {
                                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                                onWheel: (ev) => {
                                    var maxNotesY = Math.max(0, notesFlickable.contentHeight - notesFlickable.height)
                                    if (maxNotesY > 0) {
                                        if (ev.pixelDelta.y !== 0) {
                                            notesFlickable.contentY = Math.max(0, Math.min(maxNotesY, notesFlickable.contentY - ev.pixelDelta.y))
                                        } else {
                                            notesFlickable.contentY = Math.max(0, Math.min(maxNotesY, notesFlickable.contentY - (ev.angleDelta.y / 120) * 40))
                                        }
                                    }
                                    ev.accepted = true
                                }
                            }
                            Text {
                                id: notesText
                                width: parent.width
                                text: page.formatReleaseNotes(page.appNotes)
                                textFormat: Text.RichText
                                color: Theme.textMuted
                                linkColor: Theme.accent
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                                wrapMode: Text.WordWrap
                                onLinkActivated: function(link) { Qt.openUrlExternally(link) }
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
                            kind: "accent"; glyph: "\uE72C"; text: I18n.t("Проверить обновления")
                            enabled: !page.appBusy
                            onClicked: App.checkAppUpdate()
                        }
                        AccentButton {
                            kind: "ghost"; glyph: "\uE896"
                            text: page.appNewVersion !== "" ? ((page.appIsDowngrade ? I18n.t("Перейти на v") : I18n.t("Скачать v")) + page.appNewVersion + I18n.t(" и установить")) : I18n.t("Скачать и установить")
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
                    Text { text: I18n.t("Ядро Xray"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        Text { text: I18n.t("Версия:"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Text { text: page.xrayVersion !== "" ? page.xrayVersion : I18n.t("не найдена"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; font.weight: Font.DemiBold }
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
                        indeterminate: page.xrayPhase === "checking" || page.xrayPercent <= 0
                        from: 0; to: 100; value: page.xrayPercent
                    }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        AccentButton {
                            kind: "accent"; glyph: "\uE72C"; text: I18n.t("Проверить обновления Xray")
                            enabled: !page.xrayBusy
                            onClicked: App.checkXrayUpdate()
                        }
                        AccentButton {
                            kind: "ghost"; glyph: "\uE896"; text: I18n.t("Обновить Xray core")
                            visible: page.xrayPhase === "available" || page.xrayPhase === "updating"
                            enabled: !page.xrayBusy && page.xrayPhase !== "uptodate" && page.xrayPhase !== "updated"
                            onClicked: App.updateXrayCore()
                        }
                        Item { Layout.fillWidth: true }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                padding: 18
                hoverable: false
                ColumnLayout {
                    width: parent.width
                    spacing: 10
                    Text { text: I18n.t("Ядро sing-box extended"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        Text { text: I18n.t("Версия:"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Text { text: page.singboxVersion !== "" ? page.singboxVersion : I18n.t("не найдена"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; font.weight: Font.DemiBold }
                        Item { Layout.fillWidth: true }
                    }
                    Text {
                        text: page.resourceStatusText(page.singboxPhase, page.singboxMessage, I18n.t("Проверьте наличие новой версии sing-box extended."), page.singboxPercent)
                        color: page.resourceStatusColor(page.singboxPhase)
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        font.weight: (page.singboxPhase === "updated" || page.singboxPhase === "error") ? Font.DemiBold : Font.Normal
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    ProgressBar {
                        Layout.fillWidth: true
                        visible: page.singboxBusy
                        indeterminate: page.singboxPhase === "checking" || page.singboxPercent <= 0
                        from: 0; to: 100; value: page.singboxPercent
                    }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        AccentButton {
                            kind: "accent"; glyph: "\uE72C"; text: I18n.t("Проверить sing-box")
                            enabled: !page.singboxBusy
                            onClicked: App.checkSingboxUpdate()
                        }
                        AccentButton {
                            kind: "ghost"; glyph: "\uE896"; text: I18n.t("Обновить sing-box")
                            visible: page.singboxPhase === "available" || page.singboxPhase === "updating"
                            enabled: !page.singboxBusy && page.singboxPhase !== "uptodate" && page.singboxPhase !== "updated"
                            onClicked: App.updateSingboxCore()
                        }
                        Item { Layout.fillWidth: true }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                padding: 18
                hoverable: false
                ColumnLayout {
                    width: parent.width
                    spacing: 10
                    Text { text: I18n.t("GeoIP и GeoSite"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }
                    Text {
                        text: page.resourceStatusText(page.geodataPhase, page.geodataMessage, I18n.t("Обновляет geoip.dat и geosite.dat для правил маршрутизации."), page.geodataPercent)
                        color: page.resourceStatusColor(page.geodataPhase)
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        font.weight: (page.geodataPhase === "updated" || page.geodataPhase === "error") ? Font.DemiBold : Font.Normal
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    ProgressBar {
                        Layout.fillWidth: true
                        visible: page.geodataBusy
                        indeterminate: page.geodataPercent <= 0
                        from: 0; to: 100; value: page.geodataPercent
                    }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        AccentButton {
                            kind: "accent"; glyph: "\uE895"; text: I18n.t("Обновить geoip/geosite")
                            enabled: !page.geodataBusy
                            onClicked: App.updateGeodataFiles()
                        }
                        Item { Layout.fillWidth: true }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                padding: 18
                hoverable: false
                ColumnLayout {
                    width: parent.width
                    spacing: 10
                    Text { text: I18n.t("droute для Discord Voice"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        Text { text: I18n.t("Версия:"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Text { text: page.drouteVersion !== "" ? page.drouteVersion : I18n.t("не найдена"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; font.weight: Font.DemiBold }
                        Item { Layout.fillWidth: true }
                    }
                    Text {
                        text: page.resourceStatusText(page.droutePhase, page.drouteMessage, I18n.t("Проверяет и обновляет компонент droute для проксирования Discord Voice."), page.droutePercent)
                        color: page.resourceStatusColor(page.droutePhase)
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        font.weight: (page.droutePhase === "updated" || page.droutePhase === "error") ? Font.DemiBold : Font.Normal
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    ProgressBar {
                        Layout.fillWidth: true
                        visible: page.drouteBusy
                        indeterminate: page.droutePhase === "checking" || page.droutePercent <= 0
                        from: 0; to: 100; value: page.droutePercent
                    }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 8
                        AccentButton {
                            kind: "accent"; glyph: "\uE72C"; text: I18n.t("Проверить droute")
                            enabled: !page.drouteBusy
                            onClicked: App.checkDrouteUpdate()
                        }
                        AccentButton {
                            kind: "ghost"; glyph: "\uE896"; text: I18n.t("Обновить droute")
                            visible: page.droutePhase === "available" || page.droutePhase === "updating"
                            enabled: !page.drouteBusy && page.droutePhase !== "uptodate" && page.droutePhase !== "updated"
                            onClicked: App.updateDroute()
                        }
                        Item { Layout.fillWidth: true }
                    }
                }
            }

            Item { Layout.preferredHeight: 4 }
        }
    }
}
