import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import App 1.0
import "."

// Servers tab.
//
// Selection is cursor-driven (no checkboxes), mirroring the original widgets UI:
//   * left click            -> select only this row (and set the drag anchor)
//   * drag                  -> range select from the anchor to the row under the cursor
//   * Ctrl + click          -> toggle a single row
//   * Shift + click         -> range select from the anchor
//   * double click          -> set this server as the ACTIVE one
// Ping / Speed act on the current selection; the "всех" buttons act on everything.
//
// Layout notes (this revision):
//   * Rows/header are sized close to the original (compact) instead of the
//     oversized 44px rows.
//   * The table scales with the window: fixed-width columns keep their size and
//     the flexible Name + Address columns absorb any extra width, so the list
//     fills wide windows and only scrolls horizontally when the window is
//     narrower than the minimum.
//   * The search box is a clearly-visible filled field with a search glyph, and
//     the original Group / Tags / Sort / Direction controls are restored.
Item {
    id: page

    // ── selection state ──────────────────────────
    property var sel: ({})        // nodeId -> true
    property int selCount: 0
    property int selRev: 0        // bumped on every selection change so delegates re-evaluate
    property int anchorRow: -1
    property int menuRow: -1
    property bool compact: App.compactMode

    // ── filtering / sorting state ───────────────────
    property string filterText: ""
    property string filterGroup: ""   // "" = all groups
    property string filterTag: ""     // "" = all tags
    property string sortKey: "manual"
    property bool sortAsc: true

    readonly property var groupModel: ["Все группы"].concat(App.groupOptions)
    readonly property var tagModel: ["Все теги"].concat(App.tagOptions)
    readonly property var sortLabels: ["Вручную", "Имя", "Группа", "Тип", "Пинг", "Скорость", "Последнее использование"]
    readonly property var sortKeys: ["manual", "name", "group", "scheme", "ping", "speed", "last"]

    // ── sizing ───────────────────────────────
    // Closer to the original compact rows (was 36/44).
    readonly property int rowH: compact ? 30 : 34
    readonly property int cellFont: 13

    // Fixed-width columns.
    readonly property int colType: 78
    readonly property int colPort: 60
    readonly property int colGroup: 120
    readonly property int colTags: 150
    readonly property int colPing: 80
    readonly property int colSpeed: 108
    readonly property int colStatus: 72
    readonly property int colLast: 140
    readonly property int leftPad: 12
    readonly property int trailPad: 12

    // Flexible columns (minimums).
    readonly property int colNameMin: 160
    readonly property int colAddrMin: 150

    readonly property int fixedCols: colType + colPort + colGroup + colTags
        + colPing + colSpeed + colStatus + colLast
    readonly property int minTableWidth: fixedCols + colNameMin + colAddrMin + leftPad + trailPad

    // Fill the viewport when wider than the minimum; scroll when narrower.
    readonly property int tableWidth: Math.max(minTableWidth, Math.floor(hflick.width))
    readonly property int flexExtra: Math.max(0, tableWidth - minTableWidth)
    readonly property int colName: colNameMin + Math.floor(flexExtra / 2)
    readonly property int colAddr: colAddrMin + (flexExtra - Math.floor(flexExtra / 2))

    // ── selection helpers ─────────────────────
    function _commit(obj, count) {
        sel = obj;
        selCount = count;
        selRev++;
    }
    function isSelected(id) { return sel[id] === true; }
    function selectedIds() { return Object.keys(sel); }
    function firstSelected() { var k = Object.keys(sel); return k.length ? k[0] : ""; }
    // Цели для тестов: выбранные узлы, или пустой список (= все) если ничего не выбрано.
    function testTargets() { return page.selCount > 0 ? page.selectedIds() : []; }
    // Активная подписка в выпадающем списке (или null).
    function selectedSub() {
        var subs = App.subscriptions;
        if (!subs || subs.length === 0) return null;
        var i = subCombo.currentIndex;
        if (i < 0 || i >= subs.length) return null;
        return subs[i];
    }
    // Подписка, выбранная в диалоге свойств (может отличаться от subCombo).
    function infoSub() {
        var subs = App.subscriptions;
        if (!subs || subs.length === 0) return null;
        var i = infoCombo.currentIndex;
        if (i < 0 || i >= subs.length) return null;
        return subs[i];
    }
    // Человекочитаемый размер: байты → КБ/МБ/ГБ/ТБ.
    function fmtBytes(n) {
        var b = Number(n);
        if (!isFinite(b) || b < 0) return String(n);
        var u = ["Б", "КБ", "МБ", "ГБ", "ТБ", "ПБ"];
        var i = 0;
        while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
        return (i === 0 ? String(Math.round(b)) : b.toFixed(2)) + " " + u[i];
    }
    // Время → локальный часовой пояс компьютера (из unix-секунд или ISO/UTC).
    function fmtTime(v) {
        if (v === undefined || v === null || v === "") return "";
        var d;
        if (typeof v === "number") d = new Date(v * 1000);
        else {
            var s = String(v).trim();
            if (/^\d{9,}$/.test(s)) d = new Date(parseInt(s, 10) * 1000);
            else d = new Date(s);
        }
        if (isNaN(d.getTime())) return String(v);
        return Qt.formatDateTime(d, "dd.MM.yyyy HH:mm");
    }
    // Строит список [метка, значение] из данных подписки.
    function infoRows(sub) {
        var rows = [];
        if (!sub) return rows;
        rows.push(["Группа", (sub.name && sub.name.length ? sub.name : "—")]);
        if (sub.url) rows.push(["URL", sub.url]);
        if (sub.updated_at) rows.push(["Обновлено", fmtTime(sub.updated_at)]);
        rows.push(["Серверов", String(sub.node_count || 0)]);
        var ui = sub.userinfo;
        if (ui && typeof ui === "object") {
            if (ui.username) rows.push(["Пользователь", String(ui.username)]);
            if (ui.userStatus) rows.push(["Статус", String(ui.userStatus)]);
            else if (ui.isActive !== undefined) rows.push(["Статус", ui.isActive ? "Активна" : "Неактивна"]);
            if (ui.daysLeft !== undefined) rows.push(["Осталось дней", String(ui.daysLeft)]);
            // Трафик: предпочитаем точные байты (форматируем), иначе готовые строки.
            var usedB = (ui.trafficUsedBytes !== undefined) ? Number(ui.trafficUsedBytes)
                      : ((ui.total !== undefined) ? Number((ui.upload || 0) + (ui.download || 0)) : undefined);
            var limitB = (ui.trafficLimitBytes !== undefined) ? Number(ui.trafficLimitBytes)
                       : ((ui.total !== undefined) ? Number(ui.total) : undefined);
            if (usedB !== undefined || limitB !== undefined) {
                var uStr = (usedB !== undefined) ? fmtBytes(usedB)
                         : (ui.trafficUsed !== undefined ? String(ui.trafficUsed) : "?");
                var lStr = (limitB !== undefined) ? fmtBytes(limitB)
                         : (ui.trafficLimit !== undefined ? String(ui.trafficLimit) : "∞");
                rows.push(["Трафик", uStr + " / " + lStr]);
            } else if (ui.trafficUsed !== undefined || ui.trafficLimit !== undefined) {
                var used = (ui.trafficUsed !== undefined ? String(ui.trafficUsed) : "?");
                var limit = (ui.trafficLimit !== undefined ? String(ui.trafficLimit) : "∞");
                rows.push(["Трафик", used + " / " + limit]);
            }
            if (ui.expiresAt) rows.push(["Истекает", fmtTime(ui.expiresAt)]);
            else if (ui.expire) rows.push(["Истекает", fmtTime(ui.expire)]);
            if (ui.trafficLimitStrategy) rows.push(["Сброс трафика", String(ui.trafficLimitStrategy)]);
        }
        return rows;
    }
    function menuNodeId() { return App.nodeIdAt(page.menuRow); }

    function selectOnly(rowIndex) {
        var id = App.nodeIdAt(rowIndex);
        if (!id) return;
        var o = {}; o[id] = true;
        anchorRow = rowIndex;
        _commit(o, 1);
    }
    function toggle(rowIndex) {
        var id = App.nodeIdAt(rowIndex);
        if (!id) return;
        var o = Object.assign({}, sel);
        if (o[id]) delete o[id]; else o[id] = true;
        anchorRow = rowIndex;
        _commit(o, Object.keys(o).length);
    }
    function selectRange(a, b) {
        if (a < 0) a = b;
        var lo = Math.min(a, b), hi = Math.max(a, b);
        var o = {};
        for (var r = lo; r <= hi; r++) {
            var id = App.nodeIdAt(r);
            if (id) o[id] = true;
        }
        _commit(o, Object.keys(o).length);
    }
    function clearSel() { anchorRow = -1; _commit({}, 0); }
    function selectAll() {
        // Учитываем активный фильтр (группа/тег/поиск): выбираем только видимые строки.
        var o = {};
        for (var i = 0; i < list.count; i++) {
            var r = App.nodeRowAt(i);
            if (!r || !r.id) continue;
            if (!page.rowVisible(r.name, r.server, r.group, r.tags)) continue;
            o[r.id] = true;
        }
        anchorRow = 0;
        _commit(o, Object.keys(o).length);
    }

    // Combined text + group + tag filter for a single row.
    function rowVisible(name, server, group, tags) {
        if (page.filterGroup !== "" && (group || "Default") !== page.filterGroup)
            return false;
        if (page.filterTag !== "" && !(tags && tags.indexOf(page.filterTag) >= 0))
            return false;
        var q = (page.filterText || "").trim().toLowerCase();
        if (!q) return true;
        if ((name || "").toLowerCase().indexOf(q) >= 0) return true;
        if ((server || "").toLowerCase().indexOf(q) >= 0) return true;
        if ((group || "").toLowerCase().indexOf(q) >= 0) return true;
        if (tags && tags.join(" ").toLowerCase().indexOf(q) >= 0) return true;
        return false;
    }

    // Apply a sort key coming from a column-header click. Three-state cycle on
    // repeated clicks of the same column: ascending -> descending -> off (back
    // to manual order, so the arrow disappears entirely). A different column
    // starts ascending. Keeps the Sort dropdown in sync so both entry points
    // agree.
    function applySort(key) {
        if (!key) return;
        if (page.sortKey === key) {
            if (page.sortAsc) {
                page.sortAsc = false;          // asc -> desc
            } else {
                page.sortKey = "manual";       // desc -> off (manual order)
                page.sortAsc = true;
            }
        } else {
            page.sortKey = key;
            page.sortAsc = true;
        }
        var idx = page.sortKeys.indexOf(page.sortKey);
        if (idx >= 0)
            sortCombo.currentIndex = idx;
        App.setNodeSort(page.sortKey, page.sortAsc);
    }

    // ── reusable cell / header text ───────────────────
    component CellText: Text {
        property int w: 80
        width: w
        height: parent ? parent.height : 0
        leftPadding: 4
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        color: Theme.textMuted
        font.family: Theme.fontFamily
        font.pixelSize: page.cellFont
    }
    component HeaderLabel: Text {
        id: hdr
        property int w: 80
        // Empty sortKey = column is not sortable (no click handler / no arrow).
        property string sortKey: ""
        readonly property bool sortable: sortKey !== ""
        readonly property bool activeSort: sortable && page.sortKey === sortKey
        width: w
        height: parent ? parent.height : 0
        leftPadding: 4
        rightPadding: activeSort ? 16 : 0
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        // Highlight the column the list is currently sorted by.
        color: activeSort ? Theme.text : Theme.textFaint
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.bold: true

        // Sort-direction chevron, shown only on the active sort column.
        Text {
            visible: hdr.activeSort
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            text: page.sortAsc ? "\uE74A" : "\uE74B"
            font.family: "Segoe Fluent Icons"
            font.pixelSize: 10
            color: Theme.textMuted
        }

        // Click a sortable header to sort by it; click again to flip direction.
        MouseArea {
            anchors.fill: parent
            enabled: hdr.sortable
            cursorShape: Qt.PointingHandCursor
            onClicked: page.applySort(hdr.sortKey)
        }
    }
    // Group / Tags / Sort dropdowns. Their pop-ups are now opaque because
    // Main.qml sets Universal.background to the opaque Theme.flyout token.
    component FilterCombo: FluentCombo {
        Layout.preferredHeight: Theme.controlHeight
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.spacing
        spacing: Theme.spacing

        // ── title + search ───────────────────────
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing

            Text {
                text: "Серверы"
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontTitle
                font.bold: true
            }
            Text {
                visible: page.selCount > 0
                text: "· выбрано: " + page.selCount
                color: Theme.textMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                Layout.alignment: Qt.AlignVCenter
            }
            Item { Layout.fillWidth: true }

            // Visible, filled search field with a search glyph.
            Rectangle {
                id: searchBox
                Layout.preferredWidth: 300
                Layout.alignment: Qt.AlignVCenter
                height: Theme.controlHeight
                radius: Theme.radiusSmall
                color: Theme.controlFill
                border.width: 1
                border.color: searchInput.activeFocus ? Theme.accent : Theme.borderSolid

                Text {
                    id: searchGlyph
                    anchors.left: parent.left
                    anchors.leftMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    text: "\uE721"
                    font.family: "Segoe Fluent Icons"
                    font.pixelSize: 14
                    color: Theme.textFaint
                }
                TextField {
                    id: searchInput
                    anchors.left: searchGlyph.right
                    anchors.leftMargin: 8
                    anchors.right: parent.right
                    anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    background: null
                    verticalAlignment: TextInput.AlignVCenter
                    placeholderText: "Поиск серверов…"
                    placeholderTextColor: Theme.textFaint
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: page.cellFont
                    onTextChanged: page.filterText = text
                }
            }
        }

        // ── filters: group / tags / sort / direction ───────────
        Flow {
            Layout.fillWidth: true
            spacing: 8

            FilterCombo {
                id: groupCombo
                Layout.preferredWidth: 160
                width: 160
                model: page.groupModel
                onActivated: page.filterGroup = (currentIndex <= 0 ? "" : currentText)
            }
            FilterCombo {
                id: tagCombo
                Layout.preferredWidth: 160
                width: 160
                model: page.tagModel
                onActivated: page.filterTag = (currentIndex <= 0 ? "" : currentText)
            }
            FilterCombo {
                id: sortCombo
                Layout.preferredWidth: 210
                width: 210
                model: page.sortLabels
                currentIndex: 0
                onActivated: {
                    page.sortKey = page.sortKeys[currentIndex];
                    App.setNodeSort(page.sortKey, page.sortAsc);
                }
            }
            AccentButton {
                kind: "ghost"
                iconOnly: true
                glyph: page.sortAsc ? "\uE74A" : "\uE74B"
                tip: page.sortAsc ? "По возрастанию" : "По убыванию"
                onClicked: {
                    page.sortAsc = !page.sortAsc;
                    App.setNodeSort(page.sortKey, page.sortAsc);
                }
            }
        }

        // ── action toolbar ───────────────────────
        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            // Действия над серверами (слева, переносятся при нехватке места)
            Flow {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                spacing: 8

                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE8B5"; text: "Импорт из буфера"; onClicked: App.importClipboard() }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE8E5"; text: "Импорт .conf"; onClicked: App.importNodeFile() }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE8B3"; text: "Выбрать все"; onClicked: page.selectAll() }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE894"; text: "Снять выбор"; enabled: page.selCount > 0; onClicked: page.clearSel() }
                // Пинг — две кнопки как в оригинале (FIF.SEND / FIF.SYNC), способ из настроек
                AccentButton { kind: "accent"; iconOnly: true; glyph: "\uE724"; text: "Пинг выбранных"; enabled: page.selCount > 0; onClicked: App.pingNodes(page.selectedIds()) }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE895"; text: "Пинг всех"; onClicked: App.pingNodes() }
                // Тест скорости — две кнопки (выбранные / все)
                AccentButton { kind: "accent"; iconOnly: true; glyph: "\uEC4A"; text: "Тест скорости выбранных"; enabled: page.selCount > 0; onClicked: App.speedTestNodes(page.selectedIds()) }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uEC4A"; text: "Тест скорости всех"; onClicked: App.speedTestNodes() }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE769"; text: "Остановить тест"; onClicked: App.cancelSpeedTest() }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE943"; text: "Экспорт outbound JSON"; enabled: page.selCount === 1; onClicked: App.saveOutboundJson(page.firstSelected()) }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE792"; text: "Экспорт runtime JSON"; enabled: page.selCount === 1; onClicked: App.saveRuntimeJson(page.firstSelected()) }
                AccentButton { kind: "danger"; iconOnly: true; glyph: "\uE74D"; text: "Удалить выбранные"; enabled: page.selCount > 0; onClicked: App.deleteNodes(page.selectedIds()) }
            }

            // Подписки (справа, в той же линии)
            RowLayout {
                Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
                spacing: 8

                AccentButton { kind: "ghost"; iconOnly: true; glyph: "\uE946"; text: "Свойства подписки"; enabled: App.subscriptions.length > 0; onClicked: infoDialog.openInfo() }
                AccentButton { kind: "accent"; iconOnly: true; glyph: "\uE8B5"; text: "Импорт подписки"; onClicked: subDialog.openNew() }
                FilterCombo {
                    id: subCombo
                    Layout.preferredWidth: 220
                    width: 220
                    enabled: App.subscriptions.length > 0
                    model: App.subscriptions.length > 0
                        ? App.subscriptions.map(function(s) { return (s.name && s.name.length ? s.name : s.url) + " (" + (s.node_count || 0) + ")"; })
                        : ["Нет подписок"]
                }
                AccentButton { kind: "ghost"; iconOnly: true; glyph: "\uE72C"; text: "Обновить подписку"; enabled: App.subscriptions.length > 0; onClicked: { var s = page.selectedSub(); if (s) App.updateSubscription(s.url) } }
                AccentButton { kind: "ghost"; iconOnly: true; glyph: "\uE895"; text: "Обновить все подписки"; enabled: App.subscriptions.length > 0; onClicked: App.updateAllSubscriptions() }
                AccentButton { kind: "danger"; iconOnly: true; glyph: "\uE74D"; text: "Удалить подписку (с серверами)"; enabled: App.subscriptions.length > 0; onClicked: { var s = page.selectedSub(); if (s) App.removeSubscription(s.url, true) } }
            }
        }

        // ── table ────────────────────────────
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Flickable {
                id: hflick
                anchors.fill: parent
                contentWidth: page.tableWidth
                contentHeight: height
                flickableDirection: Flickable.HorizontalFlick
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.horizontal: FluentScrollBar {
                    id: hbar
                    parent: hflick
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    interactive: true
                    policy: page.tableWidth > hflick.width ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
                }

                Column {
                    width: page.tableWidth
                    height: hflick.height

                    // header
                    Rectangle {
                        id: headerRow
                        width: page.tableWidth
                        height: 30
                        color: Theme.micaBase
                        Row {
                            anchors.fill: parent
                            anchors.leftMargin: page.leftPad
                            HeaderLabel { w: page.colName; text: "Сервер"; sortKey: "name" }
                            HeaderLabel { w: page.colType; text: "Тип"; sortKey: "scheme" }
                            HeaderLabel { w: page.colAddr; text: "Адрес" }
                            HeaderLabel { w: page.colPort; text: "Порт" }
                            HeaderLabel { w: page.colGroup; text: "Группа"; sortKey: "group" }
                            HeaderLabel { w: page.colTags; text: "Теги" }
                            HeaderLabel { w: page.colPing; text: "Пинг"; sortKey: "ping" }
                            HeaderLabel { w: page.colSpeed; text: "Скорость"; sortKey: "speed" }
                            HeaderLabel { w: page.colStatus; text: "Статус" }
                            HeaderLabel { w: page.colLast; text: "Последнее"; sortKey: "last" }
                        }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.divider }
                    }

                    ListView {
                        id: list
                        width: page.tableWidth
                        height: parent.height - headerRow.height
                        model: App.nodeModel
                        // Model reset / add / remove: drop selection entries whose
                        // nodes no longer exist so selCount (and every toolbar /
                        // context-menu "enabled" + counter) stays accurate.
                        onCountChanged: {
                            if (page.selCount === 0)
                                return;
                            var o = {};
                            var n = 0;
                            for (var i = 0; i < count; i++) {
                                var id = App.nodeIdAt(i);
                                if (id && page.sel[id] === true) { o[id] = true; n++; }
                            }
                            if (n !== page.selCount) {
                                page.anchorRow = -1;
                                page._commit(o, n);
                            }
                        }
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: FluentScrollBar {}

                        // Плавная и более быстрая прокрутка колёсиком мыши (~3 строки на щелчок, glide как в FluentScroll); тачпад остаётся попиксельным.
                        NumberAnimation {
                            id: listScrollAnim
                            target: list
                            property: "contentY"
                            duration: 420
                            easing.type: Easing.OutQuint
                        }
                        WheelHandler {
                            acceptedDevices: PointerDevice.Mouse
                            onWheel: (ev) => {
                                var maxY = Math.max(0, list.contentHeight - list.height)
                                if (maxY <= 0) { ev.accepted = true; return }
                                var step = Math.max(60, page.rowH * 3)
                                var base = listScrollAnim.running ? listScrollAnim.to : list.contentY
                                var target = Math.max(0, Math.min(maxY, base - (ev.angleDelta.y / 120) * step))
                                listScrollAnim.to = target
                                listScrollAnim.restart()
                                ev.accepted = true
                            }
                        }

                        delegate: Rectangle {
                            id: nodeRow
                            required property int index
                            required property string nodeId
                            required property string name
                            required property string scheme
                            required property string server
                            required property int port
                            required property string group
                            required property var tags
                            required property int ping
                            required property real speed
                            required property bool isAlive
                            required property bool selected
                            required property int speedProgress
                            required property string flagOrient
                            required property var flagColors
                            required property string lastUsed

                            readonly property bool picked: (page.selRev >= 0) && (page.sel[nodeId] === true)
                            readonly property bool visibleRow: page.rowVisible(name, server, group, tags)

                            width: page.tableWidth
                            height: visibleRow ? page.rowH : 0
                            visible: visibleRow
                            color: picked ? Theme.accentSoft : (hover.hovered ? Theme.cardHover : "transparent")

                            HoverHandler { id: hover }

                            // active-node accent bar
                            Rectangle {
                                visible: nodeRow.selected
                                width: 3
                                height: parent.height
                                color: Theme.accent
                            }
                            // bottom divider
                            Rectangle {
                                anchors.bottom: parent.bottom
                                width: parent.width
                                height: 1
                                color: Theme.divider
                                opacity: 0.5
                            }

                            MouseArea {
                                anchors.fill: parent
                                hoverEnabled: true
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                property bool dragging: false
                                onPressed: (mouse) => {
                                    if (mouse.button === Qt.RightButton) {
                                        if (!nodeRow.picked) page.selectOnly(nodeRow.index);
                                        page.menuRow = nodeRow.index;
                                        ctxMenu.popup();
                                        return;
                                    }
                                    if (mouse.modifiers & Qt.ControlModifier) {
                                        page.toggle(nodeRow.index);
                                    } else if (mouse.modifiers & Qt.ShiftModifier) {
                                        page.selectRange(page.anchorRow < 0 ? nodeRow.index : page.anchorRow, nodeRow.index);
                                    } else {
                                        page.selectOnly(nodeRow.index);
                                        dragging = true;
                                    }
                                }
                                onPositionChanged: (mouse) => {
                                    if (!dragging) return;
                                    var pt = mapToItem(list.contentItem, mouse.x, mouse.y);
                                    var r = list.indexAt(pt.x, pt.y);
                                    if (r >= 0) page.selectRange(page.anchorRow, r);
                                }
                                onReleased: dragging = false
                                onCanceled: dragging = false
                                onDoubleClicked: App.selectNode(nodeRow.nodeId)
                            }

                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: page.leftPad

                                // name + flag
                                Item {
                                    width: page.colName
                                    height: parent.height
                                    Row {
                                        anchors.left: parent.left
                                        anchors.leftMargin: 4
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 8
                                        Item {
                                            id: flagBox
                                            width: (nodeRow.flagColors && nodeRow.flagColors.length > 0) ? 22 : 0
                                            height: 14
                                            visible: width > 0
                                            clip: true
                                            Column {
                                                anchors.fill: parent
                                                visible: nodeRow.flagOrient === "h"
                                                Repeater {
                                                    model: (nodeRow.flagOrient === "h" && nodeRow.flagColors) ? nodeRow.flagColors : []
                                                    delegate: Rectangle {
                                                        required property string modelData
                                                        width: flagBox.width
                                                        height: flagBox.height / Math.max(1, nodeRow.flagColors.length)
                                                        color: modelData
                                                    }
                                                }
                                            }
                                            Row {
                                                anchors.fill: parent
                                                visible: nodeRow.flagOrient === "v"
                                                Repeater {
                                                    model: (nodeRow.flagOrient === "v" && nodeRow.flagColors) ? nodeRow.flagColors : []
                                                    delegate: Rectangle {
                                                        required property string modelData
                                                        width: flagBox.width / Math.max(1, nodeRow.flagColors.length)
                                                        height: flagBox.height
                                                        color: modelData
                                                    }
                                                }
                                            }
                                            Rectangle {
                                                anchors.fill: parent
                                                color: "transparent"
                                                radius: 2
                                                border.width: 1
                                                border.color: Qt.rgba(0, 0, 0, 0.25)
                                            }
                                        }
                                        Text {
                                            text: nodeRow.name
                                            color: Theme.text
                                            font.family: Theme.fontFamily
                                            font.pixelSize: page.cellFont
                                            elide: Text.ElideRight
                                            width: page.colName - 24 - flagBox.width
                                            verticalAlignment: Text.AlignVCenter
                                            anchors.verticalCenter: parent.verticalCenter
                                        }
                                    }
                                }

                                CellText { w: page.colType; text: nodeRow.scheme }
                                CellText { w: page.colAddr; text: nodeRow.server; color: Theme.text }
                                CellText { w: page.colPort; text: nodeRow.port > 0 ? ("" + nodeRow.port) : "—" }
                                CellText { w: page.colGroup; text: nodeRow.group || "—" }
                                CellText { w: page.colTags; text: (nodeRow.tags && nodeRow.tags.length) ? nodeRow.tags.join(", ") : "—" }

                                // ping
                                Item {
                                    width: page.colPing
                                    height: parent.height
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        leftPadding: 4
                                        text: nodeRow.ping < 0 ? "—" : (nodeRow.ping + " ms")
                                        color: nodeRow.ping < 0 ? Theme.textFaint : Theme.pingColor(nodeRow.ping)
                                        font.family: Theme.fontFamily
                                        font.pixelSize: page.cellFont
                                    }
                                }
                                // speed
                                Item {
                                    width: page.colSpeed
                                    height: parent.height
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        leftPadding: 4
                                        text: nodeRow.speedProgress >= 0
                                            ? (nodeRow.speedProgress + "%")
                                            : (nodeRow.speed < 0 ? "—" : (nodeRow.speed.toFixed(1) + " MB/s"))
                                        color: nodeRow.speedProgress >= 0
                                            ? Theme.accent
                                            : (nodeRow.speed < 0 ? Theme.textFaint : Theme.text)
                                        font.family: Theme.fontFamily
                                        font.pixelSize: page.cellFont
                                    }
                                }
                                // status
                                Item {
                                    width: page.colStatus
                                    height: parent.height
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        leftPadding: 4
                                        text: nodeRow.isAlive ? "OK" : ((nodeRow.ping >= 0 || nodeRow.speed >= 0) ? "✕" : "—")
                                        color: nodeRow.isAlive ? Theme.success : ((nodeRow.ping >= 0 || nodeRow.speed >= 0) ? Theme.danger : Theme.textFaint)
                                        font.family: Theme.fontFamily
                                        font.pixelSize: page.cellFont
                                        font.bold: true
                                    }
                                }
                                CellText { w: page.colLast; text: nodeRow.lastUsed }
                            }
                        }
                    }
                }
            }

            // empty state
            ColumnLayout {
                anchors.centerIn: parent
                visible: list.count === 0
                spacing: 6
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "\uE9D9"
                    font.family: "Segoe Fluent Icons"
                    font.pixelSize: 40
                    color: Theme.textFaint
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "Список серверов пуст"
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontNormal
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "Скопируйте ссылки и нажмите «Импорт»"
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                }
            }
        }
    }

    // ── context menu (opaque, sized close to the original RoundMenu) ────
    Menu {
        id: ctxMenu
        width: 230
        padding: 5
        background: Rectangle {
            color: Theme.flyout
            radius: 8
            border.width: 1
            border.color: Theme.flyoutBorder
        }

        component Ctx: MenuItem {
            id: mi
            implicitHeight: 30
            contentItem: Text {
                leftPadding: 12
                rightPadding: 12
                text: mi.text
                color: mi.enabled ? Theme.text : Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: page.cellFont
                verticalAlignment: Text.AlignVCenter
            }
            background: Rectangle {
                radius: 6
                color: mi.highlighted ? Theme.cardHover : "transparent"
            }
        }
        component Sep: MenuSeparator {
            padding: 6
            topPadding: 5
            bottomPadding: 5
            background: Item {}
            contentItem: Rectangle { implicitHeight: 1; color: Theme.divider }
        }

        Ctx {
            text: "Установить как активный"
            enabled: page.selCount === 1
            onTriggered: App.selectNode(page.menuNodeId())
        }
        Sep {}
        Ctx {
            text: "Редактировать"
            enabled: page.selCount === 1
            onTriggered: editDialog.openFor(page.firstSelected())
        }
        Ctx {
            text: "Копировать ссылку"
            enabled: page.selCount === 1
            onTriggered: App.copyNodeLink(page.firstSelected())
        }
        Ctx {
            text: "Массовое редактирование (" + page.selCount + ")"
            enabled: page.selCount > 0
            onTriggered: bulkDialog.openFor(page.selectedIds())
        }
        Sep {}
        Ctx {
            text: "Пинг — способ из настроек (" + page.selCount + ")"
            enabled: page.selCount > 0
            onTriggered: App.pingNodes(page.selectedIds())
        }
        Ctx {
            text: "Тест TCP ping (" + page.selCount + ")"
            enabled: page.selCount > 0
            onTriggered: App.tcpingNodes(page.selectedIds())
        }
        Ctx {
            text: "Тест реальной задержки (" + page.selCount + ")"
            enabled: page.selCount > 0
            onTriggered: App.realDelayNodes(page.selectedIds())
        }
        Ctx {
            text: "Тест скорости загрузки (" + page.selCount + ")"
            enabled: page.selCount > 0
            onTriggered: App.downloadSpeedNodes(page.selectedIds())
        }
        Sep {}
        Ctx {
            text: "В начало списка"
            enabled: page.selCount === 1
            onTriggered: App.reorderNode(page.menuNodeId(), "top")
        }
        Ctx {
            text: "В конец списка"
            enabled: page.selCount === 1
            onTriggered: App.reorderNode(page.menuNodeId(), "bottom")
        }
        Sep {}
        Ctx {
            text: "Удалить (" + page.selCount + ")"
            enabled: page.selCount > 0
            onTriggered: App.deleteNodes(page.selectedIds())
        }
    }

    // ── keyboard shortcuts ───────────────────────
    // Only active while the Servers tab is the visible page AND the user is not
    // typing in the search field, a modal edit dialog, or the context menu, so
    // they never hijack normal text editing (Ctrl+A / Ctrl+C / Ctrl+V) inside
    // those inputs.
    readonly property bool _kbReady: page.visible
        && !searchInput.activeFocus
        && !editDialog.opened
        && !bulkDialog.opened
        && !ctxMenu.opened

    Shortcut {                              // Ctrl+V → импорт из буфера
        sequences: [ StandardKey.Paste ]
        enabled: page._kbReady
        autoRepeat: false
        onActivated: App.importClipboard()
    }
    Shortcut {                              // Ctrl+A → выбрать все
        sequences: [ StandardKey.SelectAll ]
        enabled: page._kbReady && list.count > 0
        autoRepeat: false
        onActivated: page.selectAll()
    }
    Shortcut {                              // Ctrl+C → копировать ссылку
        sequences: [ StandardKey.Copy ]
        enabled: page._kbReady && page.selCount === 1
        autoRepeat: false
        onActivated: App.copyNodeLink(page.firstSelected())
    }
    Shortcut {                              // Delete → удалить выбранные
        sequences: [ StandardKey.Delete ]
        enabled: page._kbReady && page.selCount > 0
        autoRepeat: false
        onActivated: App.deleteNodes(page.selectedIds())
    }
    Shortcut {                              // Esc → снять выбор
        sequences: [ StandardKey.Cancel ]
        enabled: page._kbReady && page.selCount > 0
        autoRepeat: false
        onActivated: page.clearSel()
    }

    // Диалоги редактирования (одиночного и массового) — порт ui/node_edit_dialog.py и ui/bulk_edit_dialog.py.
    NodeEditDialog { id: editDialog }
    BulkEditDialog { id: bulkDialog }

    // ── диалог импорта подписки (URL + имя группы) ──────────────
    Dialog {
        id: subDialog
        modal: true
        dim: true
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: 480
        padding: 18
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        function openNew() {
            subUrlField.text = "";
            subNameField.text = "";
            subDialog.open();
            subUrlField.forceActiveFocus();
        }

        background: Rectangle {
            color: Theme.flyout
            radius: 10
            border.width: 1
            border.color: Theme.flyoutBorder
        }

        contentItem: ColumnLayout {
            spacing: 12

            Text {
                text: "Импорт подписки"
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontStrong
                font.bold: true
            }
            Text {
                text: "Вставьте ссылку на подписку. Серверы автоматически попадут в новую группу."
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Text { text: "URL подписки"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            TextField {
                id: subUrlField
                Layout.fillWidth: true
                implicitHeight: Theme.controlHeight
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                selectByMouse: true
                leftPadding: 10; rightPadding: 10
                placeholderText: "https://…"
                placeholderTextColor: Theme.textFaint
                background: Rectangle { radius: Theme.radiusSmall; color: Theme.card; border.width: 1; border.color: subUrlField.activeFocus ? Theme.accent : Theme.borderSolid }
            }

            Text { text: "Имя группы (необязательно)"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            TextField {
                id: subNameField
                Layout.fillWidth: true
                implicitHeight: Theme.controlHeight
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                selectByMouse: true
                leftPadding: 10; rightPadding: 10
                placeholderText: "Пусто — имя будет создано автоматически"
                placeholderTextColor: Theme.textFaint
                background: Rectangle { radius: Theme.radiusSmall; color: Theme.card; border.width: 1; border.color: subNameField.activeFocus ? Theme.accent : Theme.borderSolid }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 6
                Item { Layout.fillWidth: true }
                AccentButton { kind: "ghost"; text: "Отмена"; onClicked: subDialog.close() }
                AccentButton {
                    kind: "accent"; text: "Импортировать"
                    enabled: subUrlField.text.trim().length > 0
                    onClicked: {
                        App.importSubscription(subUrlField.text.trim(), subNameField.text.trim());
                        subDialog.close();
                    }
                }
            }
        }
    }

    // ── диалог свойств подписки (инфо из ссылки, если есть) ──────
    Dialog {
        id: infoDialog
        modal: true
        dim: true
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: 520
        padding: 18
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        function openInfo() {
            if (!App.subscriptions || App.subscriptions.length === 0) return;
            var i = subCombo.currentIndex;
            if (i < 0 || i >= App.subscriptions.length) i = 0;
            infoCombo.currentIndex = i;
            infoDialog.open();
            // Подтягиваем свежие показатели из ссылки (прокси/VPN не трогаем).
            var s = App.subscriptions[i];
            if (s && s.url) App.updateSubscription(s.url);
        }

        background: Rectangle {
            color: Theme.flyout
            radius: 10
            border.width: 1
            border.color: Theme.flyoutBorder
        }

        contentItem: ColumnLayout {
            spacing: 12

            Text {
                text: "Свойства подписки"
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontStrong
                font.bold: true
            }

            // Переключатель подписки
            FilterCombo {
                id: infoCombo
                Layout.fillWidth: true
                enabled: App.subscriptions.length > 0
                model: App.subscriptions.length > 0
                    ? App.subscriptions.map(function(s) { return (s.name && s.name.length ? s.name : s.url) + " (" + (s.node_count || 0) + ")"; })
                    : ["Нет подписок"]
            }

            // Свойства выбранной подписки
            Column {
                Layout.fillWidth: true
                spacing: 6
                Repeater {
                    model: page.infoRows(page.infoSub())
                    delegate: RowLayout {
                        width: parent ? parent.width : 0
                        spacing: 12
                        Text {
                            text: modelData[0]
                            color: Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            Layout.preferredWidth: 140
                        }
                        Text {
                            text: modelData[1]
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            wrapMode: Text.WrapAnywhere
                            Layout.fillWidth: true
                        }
                    }
                }
            }

            Text {
                visible: { var s = page.infoSub(); return s ? !(s.userinfo && typeof s.userinfo === "object" && Object.keys(s.userinfo).length > 0) : false; }
                text: "В ссылке этой подписки нет доп. информации о пользователе."
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 6
                Item { Layout.fillWidth: true }
                AccentButton { kind: "accent"; text: "Закрыть"; onClicked: infoDialog.close() }
            }
        }
    }
}
