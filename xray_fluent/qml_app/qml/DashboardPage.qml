import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

//   • Подключение   — state, engine line, VPN/proxy switches (Вкл/Выкл),
//                     ▶/⏸ start/stop button, «Маршрут» quick row, status + target.
//   • Маршрутизация — mode controls and status caption,
//                     Discord Voice switch (ON/OFF) + hint, dns/rules/bypass info.
//   • Трафик       — Загрузка/Выгрузка/RTT stacked, sparkline, peak.
//   • Процессы     — only when the backend reports per-process stats (TUN).
FluentScroll {
    id: page
    readonly property bool twoColumns: width >= 960

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
    function fmtConns(conn, total) {
        return (total > conn) ? (conn + " (" + total + ")") : ("" + conn);
    }
    function modeTitle(m) {
        return m === "global" ? I18n.t("Глобальный") : (m === "direct" ? I18n.t("Прямой") : I18n.t("Правила"));
    }
    function stateTitle() {
        if (App.transitionBusy)
            return App.transitionTargetConnected ? I18n.t("Подключение…") : I18n.t("Отключение…");
        return App.connected ? I18n.t("Подключено") : I18n.t("Ожидание");
    }
    function engineText() {
        if (App.tunMode)
            return "VPN (TUN) → sing-box";
        return App.proxyEnabled ? I18n.t("Системный прокси Windows") : I18n.t("Прямое подключение");
    }
    readonly property bool singbox: App.tunMode && App.tunEngine === "singbox"

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
            Text {
                text: App.selectedNodeName.length > 0
                      ? (I18n.t("Готов к запуску: ") + App.selectedNodeName)
                      : I18n.t("Сервер не выбран")
                color: Theme.textMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
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
                elevation: 1
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
                    Text {
                        text: page.engineText()
                        color: Theme.textMuted; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontNormal
                        Layout.fillWidth: true; Layout.preferredWidth: 0; wrapMode: Text.WordWrap
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
                        Rectangle {
                            width: 28; height: 28; radius: 14
                            color: modeHelpHover.hovered ? Theme.cardHover : Theme.controlFill
                            border.width: 1; border.color: Theme.borderSolid
                            Text {
                                anchors.centerIn: parent
                                text: "?"
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontNormal
                                font.weight: Font.Bold
                            }
                            HoverHandler { id: modeHelpHover }
                            TapHandler { onTapped: modeHelp.open() }
                            ToolTip.visible: modeHelpHover.hovered
                            ToolTip.text: I18n.t("Чем отличаются TUN и системный прокси")
                        }
                    }
                    Popup {
                        id: modeHelp
                        width: Math.min(460, page.width - 48)
                        padding: 16
                        modal: false
                        focus: true
                        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
                        background: Rectangle {
                            color: Theme.flyout
                            border.width: 1
                            border.color: Theme.flyoutBorder
                            radius: Theme.radius
                        }
                        contentItem: ColumnLayout {
                            spacing: 10
                            Text {
                                text: I18n.t("Системный прокси")
                                color: Theme.text; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                            }
                            Text {
                                Layout.fillWidth: true
                                text: I18n.t("Работает с браузерами, мессенджерами и приложениями, которые используют настройки прокси Windows. Обычно запускается быстрее, но чаще не действует в играх и программах без поддержки прокси.")
                                color: Theme.textMuted; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontNormal; wrapMode: Text.WordWrap
                            }
                            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.divider }
                            Text {
                                text: "VPN (TUN)"
                                color: Theme.text; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                            }
                            Text {
                                Layout.fillWidth: true
                                text: I18n.t("Перехватывает трафик почти всех приложений, включая игры, и применяет правила маршрутизации на уровне системы. Требует прав администратора и запускается немного дольше.")
                                color: Theme.textMuted; font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontNormal; wrapMode: Text.WordWrap
                            }
                        }
                    }
                    // start / stop
                    AccentButton {
                        Layout.topMargin: 2
                        kind: App.connected ? "danger" : "accent"
                        enabled: !App.transitionBusy
                        glyph: App.connected ? "\uE769" : "\uE768"  // Pause / Play
                        text: (App.connected ? I18n.t("Остановить ") : I18n.t("Запустить ")) + (App.tunMode ? "VPN" : I18n.t("прокси"))
                        onClicked: App.toggleConnection()
                    }
                    // quick route presets
                    RowLayout {
                        Layout.topMargin: 2
                        spacing: 8
                        Text { text: I18n.t("Маршрут:"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        AccentButton { kind: "ghost"; text: I18n.t("Всё"); onClicked: App.applyRoutingPreset("global") }
                        AccentButton { kind: "ghost"; text: I18n.t("Блок"); onClicked: App.applyRoutingPreset("blocked") }
                        AccentButton { kind: "ghost"; text: I18n.t("Кроме РФ"); onClicked: App.applyRoutingPreset("except_ru") }
                        Item { Layout.fillWidth: true }
                    }
                    // Discord Voice (moved here from the old Routing card)
                    Rectangle { Layout.fillWidth: true; Layout.topMargin: 4; height: 1; color: Theme.divider }
                    RowLayout {
                        Layout.topMargin: 2
                        spacing: 8
                        Text {
                            text: "\uE767"
                            font.family: "Segoe Fluent Icons"
                            color: App.discordProxy ? Theme.accent : Theme.textMuted
                            font.pixelSize: Theme.fontNormal
                        }
                        Text { text: "Discord Voice"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Switch {
                            checked: App.discordProxy
                            onToggled: App.setDiscordProxy(checked)
                        }
                        Text { text: App.discordProxy ? I18n.t("Вкл") : I18n.t("Выкл"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        Item { Layout.fillWidth: true }
                    }
                    Text {
                        text: I18n.t("Голос и стримы Discord через SOCKS5 без TUN")
                        color: Theme.textFaint; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    Item { Layout.fillHeight: true; Layout.preferredHeight: 2 }
                    // status + target
                    Text {
                        text: App.runtimeMessage.length > 0 ? I18n.t(App.runtimeMessage) : (App.connected ? I18n.t("Подключено") : I18n.t("Отключено"))
                        color: Theme.textMuted; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    Text {
                        text: App.selectedNodeName.length > 0 ? App.selectedNodeName : I18n.t("Сервер не выбран")
                        color: Theme.textFaint; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.fillWidth: true; elide: Text.ElideRight
                    }
                }
            }

            // ===== Traffic card =====
            Card {
                Layout.fillWidth: true
                Layout.columnSpan: page.twoColumns ? 2 : 1
                padding: 16
                elevation: 1
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
                    TrafficGraph { id: graph; Layout.fillWidth: true; Layout.topMargin: 4 }
                    Text { id: peakLabel; text: I18n.t("Пик: 0 B/s"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                }
            }

            Card {
                id: procCard
                Layout.fillWidth: true
                Layout.columnSpan: page.twoColumns ? 2 : 1
                padding: 16
                visible: procRepeater.count > 0
                readonly property int colProc: 170
                readonly property int colSpeed: 152
                readonly property int colVpn: 78
                readonly property int colDirect: 78
                readonly property int colConns: 72
                readonly property int colTotal: 80
                ColumnLayout {
                    width: parent.width
                    spacing: 6
                    Text {
                        text: I18n.t("Трафик по процессам")
                        color: Theme.text; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                    }
                    // ---- header row ----
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: 2
                        spacing: 10
                        Text { text: I18n.t("Процесс"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colProc; elide: Text.ElideRight }
                        Text { text: I18n.t("Скорость"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colSpeed; horizontalAlignment: Text.AlignRight }
                        Text { text: "VPN"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colVpn; horizontalAlignment: Text.AlignRight }
                        Text { text: I18n.t("Прямой"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colDirect; horizontalAlignment: Text.AlignRight }
                        Text { text: I18n.t("Соед."); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colConns; horizontalAlignment: Text.AlignRight }
                        Text { text: I18n.t("Хост"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.fillWidth: true; elide: Text.ElideRight }
                        Text { text: I18n.t("Всего"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colTotal; horizontalAlignment: Text.AlignRight }
                    }
                    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.divider }
                    // ---- data rows ----
                    Repeater {
                        id: procRepeater
                        model: App.processModel
                        delegate: RowLayout {
                            required property string name
                            required property real downBps
                            required property real upBps
                            required property real proxyBytes
                            required property real directBytes
                            required property int connections
                            required property int totalConnections
                            required property string topHost
                            required property real total
                            Layout.fillWidth: true
                            spacing: 10
                            Text { text: I18n.t(name); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.preferredWidth: procCard.colProc; elide: Text.ElideRight }
                            Text { text: "↓ " + page.fmtSpeed(downBps) + "  ↑ " + page.fmtSpeed(upBps); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.preferredWidth: procCard.colSpeed; horizontalAlignment: Text.AlignRight }
                            Text { text: page.fmtBytes(proxyBytes); color: proxyBytes > 0 ? Theme.success : Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.preferredWidth: procCard.colVpn; horizontalAlignment: Text.AlignRight }
                            Text { text: page.fmtBytes(directBytes); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.preferredWidth: procCard.colDirect; horizontalAlignment: Text.AlignRight }
                            Text { text: page.fmtConns(connections, totalConnections); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.preferredWidth: procCard.colConns; horizontalAlignment: Text.AlignRight }
                            Text { text: topHost; color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.fillWidth: true; elide: Text.ElideRight }
                            Text { text: page.fmtBytes(total); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.preferredWidth: procCard.colTotal; horizontalAlignment: Text.AlignRight }
                        }
                    }
                }
            }
        }
    }

    // ---- live metric updates -----------------------------------------
    Connections {
        target: App
        function onMetricsChanged() {
            downLabel.text = I18n.t("Загрузка: ") + page.fmtSpeed(App.downBps);
            upLabel.text   = I18n.t("Выгрузка: ") + page.fmtSpeed(App.upBps);
            rttLabel.text  = "RTT: " + page.fmtLatency(App.latencyMs);
            graph.push(App.downBps, App.upBps);
            peakLabel.text = I18n.t("Пик: ") + page.fmtSpeed(graph.peak);
        }
        function onConnectedChanged() {
            if (!App.connected) {
                graph.reset();
                downLabel.text = I18n.t("Загрузка: 0 B/s");
                upLabel.text = I18n.t("Выгрузка: 0 B/s");
                rttLabel.text = "RTT: -- ";
                peakLabel.text = I18n.t("Пик: 0 B/s");
            }
        }
    }
}
