import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

//   • Подключение   — state, VPN/proxy switches (Вкл/Выкл), start/stop button.
//   • Маршрутизация — default and custom routing presets.
//   • Трафик       — Загрузка/Выгрузка/RTT stacked, sparkline, peak.
//   • Приложения   — one compact view for proxy and TUN process statistics.
FluentScroll {
    id: page
    roundedClip: false
    readonly property bool twoColumns: width >= 960
    property string processSortKey: "total"
    property bool processSortAscending: false

    function setProcessSort(key) {
        if (processSortKey === key)
            processSortAscending = !processSortAscending;
        else {
            processSortKey = key;
            processSortAscending = false;
        }
        App.setProcessTrafficSort(processSortKey, processSortAscending);
    }
    function processSortSuffix(key) {
        return processSortKey === key ? (processSortAscending ? "  ↑" : "  ↓") : "";
    }

    // ---- formatting helpers (match dashboard_page.py) ----------------
    function fmtSpeed(bps) {
        if (bps < 1024) return Math.round(bps) + " B/s";
        if (bps < 1048576) return (bps / 1024).toFixed(2) + " KB/s";
        if (bps < 1073741824) return (bps / 1048576).toFixed(2) + " MB/s";
        return (bps / 1073741824).toFixed(2) + " GB/s";
    }
    function fmtLatency(ms) { return ms < 0 ? "-- " : ms + " ms"; }
    function fmtBytes(b) {
        if (b < 1024) return Math.round(b) + " B";
        if (b < 1048576) return (b / 1024).toFixed(1) + " KB";
        if (b < 1073741824) return (b / 1048576).toFixed(1) + " MB";
        return (b / 1073741824).toFixed(2) + " GB";
    }
    function processLabel(value) {
        var name = String(value || "").replace(/\\/g, "/").split("/").pop();
        return name || I18n.t("Приложение");
    }
    function modeTitle(m) {
        return m === "global" ? I18n.t("Глобальный") : (m === "direct" ? I18n.t("Прямой") : I18n.t("Правила"));
    }
    function stateTitle() {
        if (App.transitionBusy)
            return App.transitionTargetConnected ? I18n.t("Подключение…") : I18n.t("Отключение…");
        return App.connected ? I18n.t("Подключено") : I18n.t("Ожидание");
    }
    function routingPresetIndex() {
        var items = App.routingPresetOptions;
        for (var i = 0; i < items.length; i++) {
            if (items[i].id === App.activeRoutingPresetId)
                return i;
        }
        return Math.max(0, Math.min(1, items.length - 1));
    }
    function connectionPrefix() {
        return App.connected ? I18n.t("Подключено: ") : I18n.t("Выбрано: ");
    }
    function connectionServerName() {
        return App.selectedNodeName.length > 0 ? App.selectedNodeName : I18n.t("Сервер не выбран");
    }
    component DashboardFlag: Item {
        id: flagBox
        readonly property bool hasSource: App.selectedNodeFlagSource.length > 0
        readonly property bool hasEmoji: App.selectedNodeFlag.length > 0
        readonly property bool imageReady: hasSource && flagImg.status === Image.Ready
        implicitWidth: (hasSource || hasEmoji) ? 24 : 0
        implicitHeight: 18
        visible: implicitWidth > 0
        clip: true

        Image {
            id: flagImg
            objectName: "dashboardFlagImage"
            anchors.fill: parent
            z: 1
            visible: flagBox.imageReady
            source: App.selectedNodeFlagSource
            fillMode: Image.PreserveAspectFit
            sourceSize.width: 72
            sourceSize.height: 54
            smooth: true
            mipmap: true
            asynchronous: true
            cache: true
        }

        Text {
            anchors.centerIn: parent
            visible: flagBox.hasEmoji
            text: App.selectedNodeFlag
            font.pixelSize: 18
            font.family: "Segoe UI Emoji"
            renderType: Text.NativeRendering
        }

        Rectangle {
            anchors.fill: parent
            visible: flagBox.hasSource || flagBox.hasEmoji
            color: "transparent"
            radius: 3
            antialiasing: true
            border.width: 1
            border.color: Qt.rgba(0, 0, 0, 0.2)
        }
    }

    ColumnLayout {
        width: page.width
        spacing: Theme.spacingLarge

        // ---- page header ---------------------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            Text {
                text: I18n.t("Панель управления")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontTitle
                font.weight: Font.DemiBold
            }
        }

        // ---- cards grid ----------------------------------------------
        GridLayout {
            Layout.fillWidth: true
            columns: page.twoColumns ? 2 : 1
            columnSpacing: Theme.spacingLarge
            rowSpacing: Theme.spacingLarge

            // ===== Connection card =====
            Card {
                Layout.fillWidth: true
                Layout.columnSpan: page.twoColumns ? 2 : 1
                Layout.preferredWidth: page.width
                Layout.alignment: Qt.AlignTop
                padding: 16
                elevation: 0
                ColumnLayout {
                    width: parent.width
                    spacing: 6
                    // hero: animated status ring + state heading
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 14
                        StatusRing {
                            active: App.connected
                            busy: App.transitionBusy
                            targetActive: App.transitionConnecting
                            Layout.alignment: Qt.AlignVCenter
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Text {
                                text: I18n.t("Подключение")
                                color: Theme.text; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                            }
                            Text {
                                text: page.stateTitle()
                                color: Theme.text; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontTitle
                            }
                        }
                    }
                    // switches row
                    RowLayout {
                        Layout.topMargin: 4
                        spacing: 20
                        RowLayout {
                            spacing: 8
                            Text { text: "VPN (TUN)"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                            Switch {
                                checked: App.tunMode
                                enabled: !App.transitionBusy
                                onToggled: App.setTun(checked)
                            }
                            Text { text: App.tunMode ? I18n.t("Вкл") : I18n.t("Выкл"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        }
                        RowLayout {
                            spacing: 8
                            Text { text: I18n.t("Сист. прокси"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                            Switch {
                                checked: App.proxyEnabled
                                enabled: !App.transitionBusy
                                onToggled: App.setProxy(checked)
                            }
                            Text { text: App.proxyEnabled ? I18n.t("Вкл") : I18n.t("Выкл"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        }
                        Item { Layout.fillWidth: true }
                    }
                    Dialog {
                        id: modeHelp
                        anchors.centerIn: Overlay.overlay
                        width: 460
                        padding: 0
                        modal: true
                        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
                        background: Rectangle {
                            color: Theme.flyout
                            border.width: 1
                            border.color: Theme.flyoutBorder
                            radius: 10
                        }
                        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.45) }
                        header: Text {
                            text: I18n.t("VPN (TUN) или системный прокси")
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontStrong
                            font.weight: Font.DemiBold
                            wrapMode: Text.WordWrap
                            leftPadding: 20; rightPadding: 20
                            topPadding: 18; bottomPadding: 6
                        }
                        contentItem: ColumnLayout {
                            spacing: 10
                            Text {
                                text: I18n.t("Системный прокси")
                                color: Theme.text; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                                leftPadding: 20; rightPadding: 20
                            }
                            Text {
                                Layout.fillWidth: true
                                text: I18n.t("Работает с браузерами, мессенджерами и приложениями, которые используют настройки прокси Windows. Обычно запускается быстрее, но чаще не действует в играх и программах без поддержки прокси.")
                                color: Theme.textMuted; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontNormal; wrapMode: Text.WordWrap
                                lineHeight: 1.25; leftPadding: 20; rightPadding: 20
                            }
                            Rectangle { Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; height: 1; color: Theme.divider }
                            Text {
                                text: "VPN (TUN)"
                                color: Theme.text; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                                leftPadding: 20; rightPadding: 20
                            }
                            Text {
                                Layout.fillWidth: true
                                text: I18n.t("Перехватывает трафик почти всех приложений, включая игры, и применяет правила маршрутизации на уровне системы. Требует прав администратора и запускается немного дольше.")
                                color: Theme.textMuted; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontNormal; wrapMode: Text.WordWrap
                                lineHeight: 1.25; leftPadding: 20; rightPadding: 20
                            }
                        }
                        footer: RowLayout {
                            spacing: 0
                            Item { Layout.fillWidth: true }
                            AccentButton {
                                kind: "accent"
                                text: I18n.t("Понятно")
                                onClicked: modeHelp.close()
                                Layout.rightMargin: 20
                                Layout.topMargin: 6
                                Layout.bottomMargin: 18
                            }
                        }
                    }
                    RowLayout {
                        Layout.topMargin: 4
                        Layout.fillWidth: true
                        spacing: 6
                        Text {
                            text: page.connectionPrefix()
                            color: Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                        }
                        DashboardFlag {
                            Layout.preferredWidth: visible ? 24 : 0
                            Layout.preferredHeight: 18
                        }
                        Text {
                            Layout.fillWidth: true
                            text: page.connectionServerName()
                            color: Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            elide: Text.ElideRight
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: 2
                        spacing: 12
                        AccentButton {
                            kind: App.connected ? "danger" : "accent"
                            enabled: !App.transitionBusy
                            glyph: App.connected ? "\uE769" : "\uE768"  // Pause / Play
                            text: (App.connected ? I18n.t("Остановить ") : I18n.t("Запустить ")) + (App.tunMode ? "VPN" : I18n.t("прокси"))
                            onClicked: App.toggleConnection()
                        }
                        FluentCombo {
                            Layout.preferredWidth: Math.min(230, Math.max(170, page.width - 330))
                            textRole: "name"
                            model: App.routingPresetOptions
                            currentIndex: page.routingPresetIndex()
                            onActivated: {
                                var item = App.routingPresetOptions[currentIndex];
                                if (item) App.applyRoutingPresetOption(item.id);
                            }
                        }
                        AccentButton {
                            kind: "ghost"
                            glyph: "\uE946"
                            text: ""
                            iconOnly: true
                            tip: I18n.t("Справка")
                            Layout.preferredWidth: 32
                            onClicked: modeHelp.open()
                        }
                        Item { Layout.fillWidth: true }
                    }
                    Item { Layout.fillHeight: true; Layout.preferredHeight: 2 }
                }
            }

            // ===== Traffic card =====
            Card {
                Layout.fillWidth: true
                Layout.columnSpan: page.twoColumns ? 2 : 1
                padding: 16
                elevation: 0
                ColumnLayout {
                    width: parent.width
                    spacing: 6
                    Text {
                        text: I18n.t("Трафик")
                        color: Theme.text; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                    }
                    Text { id: downLabel; text: I18n.t("Загрузка: 0 B/s"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                    Text { id: upLabel; text: I18n.t("Выгрузка: 0 B/s"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                    Text { id: rttLabel; text: "RTT: -- "; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                    Text {
                        id: totalLabel
                        text: I18n.t("Трафик сессии: ") + page.fmtBytes(App.sessionTrafficTotal)
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontNormal
                        HoverHandler { id: totalHover }
                        FluentToolTip {
                            visible: totalHover.hovered
                            text: "↓ " + I18n.t("Загрузка") + ": " + page.fmtBytes(App.sessionDownloadTotal)
                                  + "\n↑ " + I18n.t("Отправка") + ": " + page.fmtBytes(App.sessionUploadTotal)
                        }
                    }
                    TrafficGraph { id: graph; Layout.fillWidth: true; Layout.topMargin: 4 }
                    Text { id: peakLabel; text: I18n.t("Пик: 0 B/s"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                }
            }

            Card {
                id: procCard
                Layout.fillWidth: true
                Layout.columnSpan: page.twoColumns ? 2 : 1
                padding: 16
                visible: App.connected || procRepeater.count > 0
                readonly property int rateWidth: Math.round(110 * Theme.fontScale)
                readonly property int totalWidth: Math.round(112 * Theme.fontScale)
                ColumnLayout {
                    width: parent.width
                    spacing: 10
                    Text {
                        text: I18n.t("Трафик приложений")
                        textFormat: Text.PlainText
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStrong
                        font.weight: Font.DemiBold
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Text {
                            Layout.fillWidth: true
                            text: I18n.t("Приложение")
                            color: Theme.textFaint
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                        }
                        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: Theme.divider }
                        Text {
                            Layout.preferredWidth: procCard.rateWidth
                            text: "↓ " + I18n.t("Загрузка") + page.processSortSuffix("download")
                            horizontalAlignment: Text.AlignRight
                            color: page.processSortKey === "download" ? Theme.accent : Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            TapHandler { onTapped: page.setProcessSort("download") }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                        }
                        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: Theme.divider }
                        Text {
                            Layout.preferredWidth: procCard.rateWidth
                            text: "↑ " + I18n.t("Отправка") + page.processSortSuffix("upload")
                            horizontalAlignment: Text.AlignRight
                            color: page.processSortKey === "upload" ? Theme.accent : Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            TapHandler { onTapped: page.setProcessSort("upload") }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                        }
                        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: Theme.divider }
                        Text {
                            Layout.preferredWidth: procCard.totalWidth
                            Layout.rightMargin: 8
                            text: I18n.t("Всего") + page.processSortSuffix("total")
                            horizontalAlignment: Text.AlignRight
                            color: page.processSortKey === "total" ? Theme.accent : Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            TapHandler { onTapped: page.setProcessSort("total") }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                        }
                    }
                    ListView {
                        id: procRepeater
                        objectName: "dashboardProcessTraffic"
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.min(count, 7) * Math.round(44 * Theme.fontScale)
                        model: App.processModel
                        clip: true
                        reuseItems: true
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: FluentScrollBar { }
                        delegate: Item {
                            id: processRow
                            required property string name
                            required property real downBps
                            required property real upBps
                            required property real total
                            required property real proxyBytes
                            required property real directBytes
                            required property real unknownBytes
                            required property real downloadTotal
                            required property real uploadTotal
                            required property int connections
                            required property string topHost
                            required property string route
                            readonly property string displayName: page.processLabel(name)
                            width: ListView.view ? ListView.view.width : 0
                            height: Math.round(44 * Theme.fontScale)
                            Rectangle {
                                anchors.fill: parent
                                anchors.rightMargin: 8
                                radius: Theme.radiusSmall
                                color: "transparent"
                            }
                            Rectangle {
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.rightMargin: 8
                                anchors.bottom: parent.bottom
                                height: 1
                                color: Theme.divider
                                opacity: 0.5
                            }
                            RowLayout {
                                anchors.fill: parent
                                anchors.rightMargin: 8
                                spacing: 10
                                Text {
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 30
                                    text: processRow.displayName
                                    textFormat: Text.PlainText
                                    elide: Text.ElideRight
                                    color: Theme.text
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontNormal
                                }
                                Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: Theme.divider }
                                Text {
                                    Layout.preferredWidth: procCard.rateWidth
                                    text: page.fmtBytes(processRow.downloadTotal)
                                    horizontalAlignment: Text.AlignRight
                                    color: processRow.downloadTotal > 0 ? Theme.success : Theme.textFaint
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontNormal
                                }
                                Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: Theme.divider }
                                Text {
                                    Layout.preferredWidth: procCard.rateWidth
                                    text: page.fmtBytes(processRow.uploadTotal)
                                    horizontalAlignment: Text.AlignRight
                                    color: processRow.uploadTotal > 0 ? Theme.text : Theme.textFaint
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontNormal
                                }
                                Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: Theme.divider }
                                Text {
                                    Layout.preferredWidth: procCard.totalWidth
                                    text: page.fmtBytes(processRow.total)
                                    horizontalAlignment: Text.AlignRight
                                    color: processRow.total > 0 ? Theme.text : Theme.textFaint
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontNormal
                                }
                            }
                        }
                    }
                    Text {
                        visible: procRepeater.count === 0
                        Layout.fillWidth: true
                        Layout.topMargin: 8
                        Layout.bottomMargin: 8
                        text: I18n.t("Статистика появится, когда приложения начнут передавать данные")
                        textFormat: Text.PlainText
                        color: Theme.textFaint
                        wrapMode: Text.WordWrap
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                    }
                }
            }
        }
    }

    // ---- live metric updates -----------------------------------------
    Connections {
        target: App
        function onMetricsChanged() {
            downLabel.text = I18n.t("Загрузка: ") + (App.trafficAvailable ? page.fmtSpeed(App.downBps) : "—");
            upLabel.text   = I18n.t("Выгрузка: ") + (App.trafficAvailable ? page.fmtSpeed(App.upBps) : "—");
            totalLabel.text = I18n.t("Трафик сессии: ") + page.fmtBytes(App.sessionTrafficTotal);
            rttLabel.text  = "RTT: " + page.fmtLatency(App.latencyMs);
            if (App.trafficAvailable) graph.push(App.downBps, App.upBps);
            peakLabel.text = I18n.t("Пик: ") + page.fmtSpeed(graph.sessionPeak);
        }
        function onConnectedChanged() {
            if (!App.connected) {
                graph.reset();
                downLabel.text = I18n.t("Загрузка: 0 B/s");
                upLabel.text = I18n.t("Выгрузка: 0 B/s");
                totalLabel.text = I18n.t("Трафик сессии: ") + page.fmtBytes(0);
                rttLabel.text = "RTT: -- ";
                peakLabel.text = I18n.t("Пик: 0 B/s");
            }
        }
    }
}
