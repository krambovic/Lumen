import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

// Dashboard — rebuilt to mirror the original qfluentwidgets dashboard_page.py
// 1:1: a page header (title + summary), then a responsive grid of cards:
//   • Подключение   — state, engine line, VPN/proxy switches (Вкл/Выкл),
//                     ▶/⏸ start/stop button, «Маршрут» quick row, status + target.
//   • Маршрутизация — mode ComboBox (enabled only for tun2socks), mode caption,
//                     Discord voice switch (ON/OFF) + hint, dns/rules/bypass info.
//   • Трафик       — Загрузка/Выгрузка/RTT stacked, sparkline, peak.
//   • Процессы     — only when the backend reports per-process stats (TUN).
FluentScroll {
    id: page
    clip: true

    // Drop to a single column a bit earlier (960 instead of 860): below this the
    // Connection card column gets too narrow and the "Выкл" label by the Сист.
    // прокси switch starts spilling over the card edge — so we stack the cards
    // (full-width) before that happens.
    readonly property bool twoColumns: width >= 960

    // ---- formatting helpers (match dashboard_page.py) ----------------
    function fmtSpeed(bps) {
        if (bps < 1024) return Math.round(bps) + " B/s";
        if (bps < 1048576) return (bps / 1024).toFixed(2) + " KB/s";
        if (bps < 1073741824) return (bps / 1048576).toFixed(2) + " MB/s";
        return (bps / 1073741824).toFixed(2) + " GB/s";
    }
    function fmtLatency(ms) { return ms < 0 ? "-- " : ms + " ms"; }
    // Cumulative byte counter formatting — mirrors dashboard_page._format_bytes:
    // integer for B, one decimal for KB/MB, two decimals for GB.
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
        return m === "global" ? "Глобальный" : (m === "direct" ? "Прямой" : "Правила");
    }
    function stateTitle() {
        if (App.transitionBusy) return "Подключение…";
        return App.connected ? "Подключено" : "Ожидание";
    }
    function engineText() {
        if (App.tunMode)
            return App.tunEngine === "singbox"
                ? "VPN (TUN) → sing-box (recommended, auto hybrid)"
                : "VPN (TUN) → tun2socks";
        return App.proxyEnabled ? "Системный прокси Windows" : "Прямое подключение";
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
                text: "Панель управления"
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontTitle
                font.weight: Font.DemiBold
            }
            Text {
                text: App.selectedNodeName.length > 0
                      ? ("Готов к запуску: " + App.selectedNodeName)
                      : "Сервер не выбран"
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
                // Fixed, equal, content-independent column width: each top-row card
                // always takes exactly half the row (minus the column gap) in two-
                // column mode, and the full width when stacked. This way toggling
                // TUN / Сист. прокси / Discord never changes the card size — the
                // grid no longer redistributes width based on each card's content.
                Layout.preferredWidth: page.twoColumns ? (page.width - Theme.spacingLarge) / 2 : page.width
                Layout.alignment: Qt.AlignTop
                padding: 16
                ColumnLayout {
                    width: parent.width
                    spacing: 6
                    Text {
                        text: "Подключение"
                        color: Theme.text; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                    }
                    Text {
                        text: page.stateTitle()
                        color: Theme.text; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontTitle
                    }
                    Text {
                        text: page.engineText()
                        color: Theme.textMuted; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontNormal
                        // preferredWidth 0 + fillWidth: the variable-length engine line
                        // wraps inside the card instead of stretching it, so the card
                        // keeps the SAME width for every TUN / сист.прокси / off combo
                        // (otherwise the long sing-box line steals width from the grid).
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
                            Text { text: App.tunMode ? "Вкл" : "Выкл"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        }
                        RowLayout {
                            spacing: 8
                            Text { text: "Сист. прокси"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                            Switch {
                                checked: App.proxyEnabled
                                // Mutually exclusive with TUN, but still toggleable
                                // FROM TUN: the bridge disables TUN and reconnects in
                                // proxy mode in one step (mirrors the original).
                                enabled: !App.transitionBusy
                                onToggled: App.setProxy(checked)
                            }
                            Text { text: App.proxyEnabled ? "Вкл" : "Выкл"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        }
                        Item { Layout.fillWidth: true }
                    }
                    // start / stop
                    AccentButton {
                        Layout.topMargin: 2
                        kind: App.connected ? "danger" : "accent"
                        enabled: !App.transitionBusy
                        glyph: App.connected ? "\uE769" : "\uE768"  // Pause / Play
                        text: (App.connected ? "Остановить " : "Запустить ") + (App.tunMode ? "VPN" : "прокси")
                        onClicked: App.toggleConnection()
                    }
                    // quick route presets
                    RowLayout {
                        Layout.topMargin: 2
                        spacing: 8
                        Text { text: "Маршрут:"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        AccentButton { kind: "ghost"; text: "Всё"; onClicked: App.applyRoutingPreset("global") }
                        AccentButton { kind: "ghost"; text: "Блок"; onClicked: App.applyRoutingPreset("blocked") }
                        AccentButton { kind: "ghost"; text: "Кроме РФ"; onClicked: App.applyRoutingPreset("except_ru") }
                        Item { Layout.fillWidth: true }
                    }
                    Item { Layout.fillHeight: true; Layout.preferredHeight: 2 }
                    // status + target
                    Text {
                        text: App.runtimeMessage.length > 0 ? App.runtimeMessage : (App.connected ? "Подключено" : "Отключено")
                        color: Theme.textMuted; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    Text {
                        text: App.selectedNodeName.length > 0 ? App.selectedNodeName : "Сервер не выбран"
                        color: Theme.textFaint; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.fillWidth: true; elide: Text.ElideRight
                    }
                }
            }

            // ===== Routing card =====
            Card {
                Layout.fillWidth: true
                // Fixed, equal, content-independent column width: each top-row card
                // always takes exactly half the row (minus the column gap) in two-
                // column mode, and the full width when stacked. This way toggling
                // TUN / Сист. прокси / Discord never changes the card size — the
                // grid no longer redistributes width based on each card's content.
                Layout.preferredWidth: page.twoColumns ? (page.width - Theme.spacingLarge) / 2 : page.width
                Layout.alignment: Qt.AlignTop
                padding: 16
                ColumnLayout {
                    width: parent.width
                    spacing: 8
                    Text {
                        text: "Маршрутизация"
                        color: Theme.text; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                    }
                    FluentCombo {
                        id: modeCombo
                        Layout.fillWidth: true
                        property var modeKeys: ["global", "rule", "direct"]
                        enabled: App.tunMode && App.tunEngine !== "singbox" && !App.transitionBusy
                        model: ["Глобальный", "Правила", "Прямой"]
                        currentIndex: Math.max(0, modeKeys.indexOf(App.routingMode))
                        onActivated: App.setRoutingMode(modeKeys[currentIndex])
                    }
                    Text {
                        text: page.singbox ? "Routing из raw sing-box config"
                                           : ("Режим: " + page.modeTitle(App.routingMode))
                        color: Theme.text; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontNormal
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    Item { Layout.fillHeight: true; Layout.preferredHeight: 2 }
                    // Discord voice
                    RowLayout {
                        spacing: 8
                        Text { text: "Discord voice"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Switch {
                            checked: App.discordProxy
                            onToggled: App.setDiscordProxy(checked)
                        }
                        Text { text: App.discordProxy ? "ON" : "OFF"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        Item { Layout.fillWidth: true }
                    }
                    Text {
                        text: "Голос и стримы Discord через SOCKS5 без TUN"
                        color: Theme.textMuted; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    Text {
                        text: page.singbox ? "DNS и routing берутся из editor JSON"
                                           : "DNS по умолчанию системный"
                        color: Theme.textFaint; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    Text {
                        text: page.singbox ? "sing-box TUN перехватывает системный трафик, поэтому process/path правила работают полноценно"
                                           : "Правила применяются выбранным движком маршрутизации"
                        color: Theme.textFaint; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                    Text {
                        text: page.singbox ? "GUI routing не влияет на sing-box: конфигурация берётся целиком из JSON"
                                           : "Bypass-список применяется к системному прокси"
                        color: Theme.textFaint; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                    }
                }
            }

            // ===== Traffic card =====
            Card {
                Layout.fillWidth: true
                Layout.columnSpan: page.twoColumns ? 2 : 1
                padding: 16
                ColumnLayout {
                    width: parent.width
                    spacing: 6
                    Text {
                        text: "Трафик"
                        color: Theme.text; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                    }
                    Text { id: downLabel; text: "Загрузка: 0 B/s"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                    Text { id: upLabel; text: "Выгрузка: 0 B/s"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                    Text { id: rttLabel; text: "RTT: -- "; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                    TrafficGraph { id: graph; Layout.fillWidth: true; Layout.topMargin: 4 }
                    Text { id: peakLabel; text: "Пик: 0 B/s"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                }
            }

            // ===== Process card (TUN only, when stats present) =====
            // Faithful port of dashboard_page.py's per-process table. Seven
            // columns: Процесс | Скорость | VPN | Прямой | Соед. | Хост | Всего.
            // Matches the original QHeaderView resize modes exactly: Процесс is a
            // fixed (interactive) width, the numeric columns size to content, and
            // ХОСТ is the single stretch column that absorbs the slack — NOT the
            // process-name column.
            Card {
                id: procCard
                Layout.fillWidth: true
                Layout.columnSpan: page.twoColumns ? 2 : 1
                padding: 16
                visible: procRepeater.count > 0
                // NOTE: these must be referenced as procCard.colX from children /
                // Repeater delegates — unqualified lookup does not reach custom
                // properties declared on an external component instance (Card).
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
                        text: "Трафик по процессам"
                        color: Theme.text; font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                    }
                    // ---- header row ----
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: 2
                        spacing: 10
                        Text { text: "Процесс"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colProc; elide: Text.ElideRight }
                        Text { text: "Скорость"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colSpeed; horizontalAlignment: Text.AlignRight }
                        Text { text: "VPN"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colVpn; horizontalAlignment: Text.AlignRight }
                        Text { text: "Прямой"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colDirect; horizontalAlignment: Text.AlignRight }
                        Text { text: "Соед."; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colConns; horizontalAlignment: Text.AlignRight }
                        Text { text: "Хост"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.fillWidth: true; elide: Text.ElideRight }
                        Text { text: "Всего"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: procCard.colTotal; horizontalAlignment: Text.AlignRight }
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
                            Text { text: name; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.preferredWidth: procCard.colProc; elide: Text.ElideRight }
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
            downLabel.text = "Загрузка: " + page.fmtSpeed(App.downBps);
            upLabel.text   = "Выгрузка: " + page.fmtSpeed(App.upBps);
            rttLabel.text  = "RTT: " + page.fmtLatency(App.latencyMs);
            graph.push(App.downBps, App.upBps);
            peakLabel.text = "Пик: " + page.fmtSpeed(graph.peak);
        }
        function onConnectedChanged() {
            if (!App.connected) {
                graph.reset();
                downLabel.text = "Загрузка: 0 B/s";
                upLabel.text = "Выгрузка: 0 B/s";
                rttLabel.text = "RTT: -- ";
                peakLabel.text = "Пик: 0 B/s";
            }
        }
    }
}
