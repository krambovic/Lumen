import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

// Traffic history — live port of ui/history_page.py.
//
// Period selector + refresh, a summary strip (Загружено / Отдано / Сессий) and
// three FIXED-HEIGHT table cards (Сессии / Трафик по дням / Трафик по
// процессам). Each card pins its header row and scrolls its body internally —
// exactly like the original SmoothScrollArea page where every TableWidget had a
// fixed minimum height and scrolled its own rows instead of growing forever.
// All figures come from App.historyData(days), mirroring HistoryPage._refresh().
Item {
    id: page

    readonly property var daysModel: [7, 30, 3650]
    property int periodDays: 30
    property var hist: ({})
    readonly property int cellFont: 13
    readonly property int rowH: 34

    function reload() { hist = App.historyData(periodDays) }
    Component.onCompleted: reload()

    // ── per-table column sort state ────────────────────────────────
    // key = "" means unsorted (natural order from the backend, no arrow).
    // Clicking a column header cycles: ascending → descending → off (3rd click).
    property string sessSort: ""
    property bool   sessAsc: true
    property string daySort: ""
    property bool   dayAsc: true
    property string procSort: ""
    property bool   procAsc: true

    // Three-state cycle for a column header click.
    function cycleSort(table, key) {
        var k, a
        if (table === "sess") { k = sessSort; a = sessAsc }
        else if (table === "day") { k = daySort; a = dayAsc }
        else { k = procSort; a = procAsc }
        if (k === key) {
            if (a) { a = false }            // asc → desc
            else { k = ""; a = true }       // desc → off (natural order)
        } else { k = key; a = true }        // new column → asc
        if (table === "sess") { sessSort = k; sessAsc = a }
        else if (table === "day") { daySort = k; dayAsc = a }
        else { procSort = k; procAsc = a }
    }

    // Sorted copy of `rows` by `key`: numbers sort numerically, everything
    // else by locale-aware string compare. key === "" → returned unchanged.
    function sortedRows(rows, key, asc) {
        if (!rows || key === "")
            return rows || []
        var out = rows.slice()
        out.sort(function(x, y) {
            var a = x[key], b = y[key]
            var r
            if (typeof a === "number" && typeof b === "number")
                r = a - b
            else
                r = ("" + a).localeCompare("" + b)
            return asc ? r : -r
        })
        return out
    }

    function modeLabel(mode) {
        if (mode === "xray")
            return I18n.t("Прокси")
        if (mode === "xray-tun")
            return I18n.t("TUN (xray experimental)")
        if (mode === "singbox")
            return I18n.t("TUN (sing-box)")
        return I18n.t(mode || "")
    }

    function routeLabel(route) {
        if (route === "proxy")
            return "VPN"
        if (route === "direct")
            return I18n.t("Прямой")
        if (route === "mixed")
            return I18n.t("Смешанный")
        return I18n.t(route || "")
    }

    component StyledCombo: FluentCombo {}

    component StatTile: Card {
        property string caption: ""
        property string value: ""
        property color valueColor: Theme.text
        Layout.fillWidth: true
        padding: 16
        hoverable: false
        ColumnLayout {
            width: parent.width
            spacing: 4
            Text { text: caption; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            Text { text: value; color: valueColor; font.family: Theme.fontFamily; font.pixelSize: Theme.fontTitle; font.weight: Font.DemiBold }
        }
    }

    // Header cell: w = 0 means "flexible / fill".
    // Clickable when sortKey is set; appends a ▲/▼ arrow while it is the
    // active sort column. curKey/curAsc come from the owning table's state.
    component HCell: Text {
        id: hcell
        property int w: 0
        property bool alignRight: false
        property string label: ""
        property string sortKey: ""
        property string curKey: ""
        property bool curAsc: true
        signal sortClicked()
        readonly property bool sortable: sortKey !== ""
        readonly property bool active: sortable && curKey === sortKey
        text: label + (active ? (curAsc ? "  ▲" : "  ▼") : "")
        color: active ? Theme.text : Theme.textMuted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.weight: Font.DemiBold
        Layout.fillWidth: w === 0
        Layout.preferredWidth: w > 0 ? w : -1
        horizontalAlignment: alignRight ? Text.AlignRight : Text.AlignLeft
        MouseArea {
            anchors.fill: parent
            enabled: hcell.sortable
            cursorShape: Qt.PointingHandCursor
            onClicked: hcell.sortClicked()
        }
    }
    // Data cell.
    component DCell: Text {
        property int w: 0
        property bool alignRight: false
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: page.cellFont
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
        Layout.fillWidth: w === 0
        Layout.preferredWidth: w > 0 ? w : -1
        horizontalAlignment: alignRight ? Text.AlignRight : Text.AlignLeft
    }

    // Thin Win11 scrollbar for table bodies (auto-hide, grows on hover).
    component ThinBar: ScrollBar {
        id: sb
        policy: ScrollBar.AsNeeded
        padding: 2
        contentItem: Rectangle {
            implicitWidth: (sb.hovered || sb.pressed) ? 8 : 4
            implicitHeight: 30
            radius: width / 2
            color: sb.pressed ? Theme.textMuted : Theme.textFaint
            opacity: sb.active ? 0.9 : 0.38
            Behavior on opacity { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
            Behavior on implicitWidth { NumberAnimation { duration: 130; easing.type: Easing.OutCubic } }
        }
        background: Rectangle { color: "transparent" }
    }

    // Scrollable table body: no bounce, thin bar, smooth wheel that hands
    // control back to the page scroll when the table itself can't scroll.
    component TableList: ListView {
        id: tl
        property real wheelStep: 92
        property int glide: 540
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: 5000
        maximumFlickVelocity: 3000
        pixelAligned: true
        ScrollBar.vertical: ThinBar {}

        // Same immersive, softly-decelerating glide as the page-level FluentScroll.
        NumberAnimation {
            id: tlScrollAnim
            target: tl; property: "contentY"
            duration: tl.glide; easing.type: Easing.OutQuint
        }

        WheelHandler {
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: (ev) => {
                var maxY = Math.max(0, tl.contentHeight - tl.height)
                if (maxY <= 0) { ev.accepted = false; return }   // hand off to the page
                if (ev.pixelDelta.y !== 0) {        // trackpad: pixel-precise, no glide
                    tlScrollAnim.stop()
                    tl.contentY = Math.max(0, Math.min(maxY, tl.contentY - ev.pixelDelta.y))
                } else {                            // mouse wheel: animated eased step
                    var base = tlScrollAnim.running ? tlScrollAnim.to : tl.contentY
                    tlScrollAnim.to = Math.max(0, Math.min(maxY, base - (ev.angleDelta.y / 120) * tl.wheelStep))
                    tlScrollAnim.restart()
                }
                ev.accepted = true
            }
        }
    }

    // Fixed-height table card: pinned header + internally scrolled body.
    component TableCard: Card {
        property int bodyHeight: 220
        Layout.fillWidth: true
        Layout.preferredHeight: bodyHeight + 52
        padding: 0
        hoverable: false
    }

    FluentScroll {
        anchors.fill: parent

        ColumnLayout {
            width: page.width
            spacing: Theme.spacingLarge

            // ---- header: title + period + refresh ----
            RowLayout {
                Layout.fillWidth: true
                Text {
                    text: I18n.t("История")
                    color: Theme.text; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontTitle; font.weight: Font.DemiBold
                }
                Item { Layout.fillWidth: true }
                StyledCombo {
                    id: periodCombo
                    Layout.preferredWidth: 180
                    model: [I18n.t("За 7 дней"), I18n.t("За 30 дней"), I18n.t("За всё время")]
                    currentIndex: 1
                    onActivated: {
                        page.periodDays = page.daysModel[currentIndex]
                        page.reload()
                    }
                }
                AccentButton { kind: "ghost"; glyph: "\uE72C"; text: I18n.t("Обновить"); onClicked: page.reload() }
            }

            // ---- summary strip ----
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacing
                StatTile { caption: I18n.t("Загружено"); value: page.hist.summaryDown || "0 B"; valueColor: Theme.success }
                StatTile { caption: I18n.t("Отдано"); value: page.hist.summaryUp || "0 B" }
                StatTile { caption: I18n.t("Сессий"); value: "" + (page.hist.summarySessions || 0) }
            }

            // ---- sessions table ----
            Text { text: I18n.t("Сессии"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold; Layout.topMargin: 4 }
            TableCard {
                bodyHeight: 250
                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0
                    RowLayout {
                        Layout.fillWidth: true; Layout.margins: 14; Layout.bottomMargin: 8; spacing: 12
                        HCell { label: I18n.t("Начало"); w: 140; sortKey: "ts"; curKey: page.sessSort; curAsc: page.sessAsc; onSortClicked: page.cycleSort("sess", "ts") }
                        HCell { label: I18n.t("Сервер"); sortKey: "node"; curKey: page.sessSort; curAsc: page.sessAsc; onSortClicked: page.cycleSort("sess", "node") }
                        HCell { label: I18n.t("Режим"); w: 150; sortKey: "mode"; curKey: page.sessSort; curAsc: page.sessAsc; onSortClicked: page.cycleSort("sess", "mode") }
                        HCell { label: I18n.t("Длит."); w: 80; alignRight: true; sortKey: "durSec"; curKey: page.sessSort; curAsc: page.sessAsc; onSortClicked: page.cycleSort("sess", "durSec") }
                        HCell { label: "↓"; w: 100; alignRight: true; sortKey: "downB"; curKey: page.sessSort; curAsc: page.sessAsc; onSortClicked: page.cycleSort("sess", "downB") }
                        HCell { label: "↑"; w: 100; alignRight: true; sortKey: "upB"; curKey: page.sessSort; curAsc: page.sessAsc; onSortClicked: page.cycleSort("sess", "upB") }
                    }
                    Rectangle { Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; height: 1; color: Theme.divider }
                    Item {
                        Layout.fillWidth: true; Layout.fillHeight: true
                        TableList {
                            id: sessionsList
                            anchors.fill: parent
                            model: page.sortedRows(page.hist.sessions || [], page.sessSort, page.sessAsc)
                            delegate: Item {
                                width: ListView.view.width
                                implicitHeight: page.rowH
                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 14; anchors.rightMargin: 14
                                    spacing: 12
                                    DCell { text: modelData.date; w: 140 }
                                    DCell { text: modelData.node }
                                    DCell { text: page.modeLabel(modelData.mode); w: 150; color: modelData.isTun ? Theme.accent : Theme.text }
                                    DCell { text: modelData.duration; w: 80; alignRight: true }
                                    DCell { text: modelData.down; w: 100; alignRight: true; color: modelData.hasDown ? Theme.success : Theme.text }
                                    DCell { text: modelData.up; w: 100; alignRight: true }
                                }
                            }
                        }
                        Text {
                            anchors.centerIn: parent
                            visible: sessionsList.count === 0
                            text: I18n.t("Нет записей"); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        }
                    }
                }
            }

            // ---- per-day traffic table ----
            Text { text: I18n.t("Трафик по дням"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold; Layout.topMargin: 4 }
            TableCard {
                bodyHeight: 180
                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0
                    RowLayout {
                        Layout.fillWidth: true; Layout.margins: 14; Layout.bottomMargin: 8; spacing: 12
                        HCell { label: I18n.t("День"); sortKey: "day"; curKey: page.daySort; curAsc: page.dayAsc; onSortClicked: page.cycleSort("day", "day") }
                        HCell { label: I18n.t("Загружено"); w: 150; alignRight: true; sortKey: "downB"; curKey: page.daySort; curAsc: page.dayAsc; onSortClicked: page.cycleSort("day", "downB") }
                        HCell { label: I18n.t("Отдано"); w: 150; alignRight: true; sortKey: "upB"; curKey: page.daySort; curAsc: page.dayAsc; onSortClicked: page.cycleSort("day", "upB") }
                    }
                    Rectangle { Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; height: 1; color: Theme.divider }
                    Item {
                        Layout.fillWidth: true; Layout.fillHeight: true
                        TableList {
                            id: dailyList
                            anchors.fill: parent
                            model: page.sortedRows(page.hist.daily || [], page.daySort, page.dayAsc)
                            delegate: Item {
                                width: ListView.view.width
                                implicitHeight: page.rowH
                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 14; anchors.rightMargin: 14
                                    spacing: 12
                                    DCell { text: modelData.day }
                                    DCell { text: modelData.down; w: 150; alignRight: true; color: modelData.hasDown ? Theme.success : Theme.text }
                                    DCell { text: modelData.up; w: 150; alignRight: true }
                                }
                            }
                        }
                        Text {
                            anchors.centerIn: parent
                            visible: dailyList.count === 0
                            text: I18n.t("Нет записей"); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        }
                    }
                }
            }

            // ---- per-process traffic table ----
            Text { text: I18n.t("Трафик по процессам"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold; Layout.topMargin: 4 }
            TableCard {
                bodyHeight: 210
                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0
                    RowLayout {
                        Layout.fillWidth: true; Layout.margins: 14; Layout.bottomMargin: 8; spacing: 12
                        HCell { label: I18n.t("Процесс"); sortKey: "exe"; curKey: page.procSort; curAsc: page.procAsc; onSortClicked: page.cycleSort("proc", "exe") }
                        HCell { label: I18n.t("Загружено"); w: 140; alignRight: true; sortKey: "downB"; curKey: page.procSort; curAsc: page.procAsc; onSortClicked: page.cycleSort("proc", "downB") }
                        HCell { label: I18n.t("Отдано"); w: 140; alignRight: true; sortKey: "upB"; curKey: page.procSort; curAsc: page.procAsc; onSortClicked: page.cycleSort("proc", "upB") }
                        HCell { label: I18n.t("Маршрут"); w: 110; alignRight: true; sortKey: "route"; curKey: page.procSort; curAsc: page.procAsc; onSortClicked: page.cycleSort("proc", "route") }
                    }
                    Rectangle { Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; height: 1; color: Theme.divider }
                    Item {
                        Layout.fillWidth: true; Layout.fillHeight: true
                        TableList {
                            id: procList
                            anchors.fill: parent
                            model: page.sortedRows(page.hist.procs || [], page.procSort, page.procAsc)
                            delegate: Item {
                                width: ListView.view.width
                                implicitHeight: page.rowH
                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 14; anchors.rightMargin: 14
                                    spacing: 12
                                    DCell { text: modelData.exe }
                                    DCell { text: modelData.down; w: 140; alignRight: true; color: modelData.hasDown ? Theme.success : Theme.text }
                                    DCell { text: modelData.up; w: 140; alignRight: true }
                                    DCell { text: page.routeLabel(modelData.route); w: 110; alignRight: true; color: Theme.textMuted }
                                }
                            }
                        }
                        Text {
                            anchors.centerIn: parent
                            visible: procList.count === 0
                            text: I18n.t("Нет записей"); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                        }
                    }
                }
            }

            Item { Layout.preferredHeight: 4 }
        }
    }
}
