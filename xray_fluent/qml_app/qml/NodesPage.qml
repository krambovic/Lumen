import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import QtQuick.Effects
import App 1.0
import "."

// Servers tab.
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

    readonly property var groupModel: [I18n.t("Все группы")].concat(App.groupOptions)
    readonly property var tagModel: [I18n.t("Все теги")].concat(App.tagOptions)
    readonly property var sortLabels: [I18n.t("Вручную"), I18n.t("Имя"), I18n.t("Группа"), I18n.t("Тип"), I18n.t("Транспорт"), I18n.t("Пинг"), I18n.t("Скорость"), I18n.t("Последнее использование")]
    readonly property var sortKeys: ["manual", "name", "group", "scheme", "transport", "ping", "speed", "last"]

    // ── sizing ───────────────────────────────
    // Closer to the original compact rows (was 36/44).
    readonly property int rowH: compact ? 30 : 34
    readonly property int cellFont: 13

    // Fixed-width columns.
    readonly property int colType: 78
    readonly property int colTransport: 92
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

    readonly property int fixedCols: colType + colTransport + colPort + colGroup + colTags
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
    function testTargets() { return page.selCount > 0 ? page.selectedIds() : []; }
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
        var u = [I18n.t("Б"), I18n.t("КБ"), I18n.t("МБ"), I18n.t("ГБ"), I18n.t("ТБ"), I18n.t("ПБ")];
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
        rows.push([I18n.t("Группа"), (sub.name && sub.name.length ? sub.name : "—")]);
        if (sub.url) rows.push(["URL", sub.url]);
        if (sub.updated_at) rows.push([I18n.t("Обновлено"), fmtTime(sub.updated_at)]);
        rows.push([I18n.t("Серверов"), String(sub.node_count || 0)]);
        var ui = sub.userinfo;
        if (ui && typeof ui === "object") {
            if (ui.username) rows.push([I18n.t("Пользователь"), String(ui.username)]);
            if (ui.userStatus) rows.push([I18n.t("Статус"), String(ui.userStatus)]);
            else if (ui.isActive !== undefined) rows.push([I18n.t("Статус"), ui.isActive ? I18n.t("Активна") : I18n.t("Неактивна")]);
            if (ui.daysLeft !== undefined) rows.push([I18n.t("Осталось дней"), String(ui.daysLeft)]);
            var usedB = (ui.trafficUsedBytes !== undefined) ? Number(ui.trafficUsedBytes)
                      : ((ui.total !== undefined) ? Number((ui.upload || 0) + (ui.download || 0)) : undefined);
            var limitB = (ui.trafficLimitBytes !== undefined) ? Number(ui.trafficLimitBytes)
                       : ((ui.total !== undefined) ? Number(ui.total) : undefined);
            if (usedB !== undefined || limitB !== undefined) {
                var uStr = (usedB !== undefined) ? fmtBytes(usedB)
                         : (ui.trafficUsed !== undefined ? String(ui.trafficUsed) : "?");
                var lStr = (limitB !== undefined) ? fmtBytes(limitB)
                         : (ui.trafficLimit !== undefined ? String(ui.trafficLimit) : "∞");
                rows.push([I18n.t("Трафик"), uStr + " / " + lStr]);
            } else if (ui.trafficUsed !== undefined || ui.trafficLimit !== undefined) {
                var used = (ui.trafficUsed !== undefined ? String(ui.trafficUsed) : "?");
                var limit = (ui.trafficLimit !== undefined ? String(ui.trafficLimit) : "∞");
                rows.push([I18n.t("Трафик"), used + " / " + limit]);
            }
            if (ui.expiresAt) rows.push([I18n.t("Истекает"), fmtTime(ui.expiresAt)]);
            else if (ui.expire) rows.push([I18n.t("Истекает"), fmtTime(ui.expire)]);
            if (ui.trafficLimitStrategy) rows.push([I18n.t("Сброс трафика"), String(ui.trafficLimitStrategy)]);
        }
        return rows;
    }
    function subscriptionMeta(sub) {
        if (!sub || !sub.userinfo || typeof sub.userinfo !== "object") return null;
        var ui = sub.userinfo;
        if (!ui.profileTitle && !ui.supportUrl && !ui.profileUrl && !ui.telegramUrl && !ui.clientProfile) return null;
        return ui;
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
        var o = {};
        for (var i = 0; i < list.count; i++) {
            var id = App.nodeIdAt(i);
            if (id) o[id] = true;
        }
        anchorRow = 0;
        _commit(o, Object.keys(o).length);
    }

    function applyFilters() {
        App.setNodeFilter(page.filterGroup, page.filterTag, page.filterText);
    }

    function revealImportedNode(nodeId) {
        if (!nodeId) return;
        page.filterText = "";
        page.filterGroup = "";
        page.filterTag = "";
        searchInput.text = "";
        groupCombo.currentIndex = 0;
        tagCombo.currentIndex = 0;
        page.applyFilters();
        Qt.callLater(function() {
            var row = App.nodeIndexById(nodeId);
            if (row < 0) return;
            page.selectOnly(row);
            list.positionViewAtIndex(row, ListView.Center);
        });
    }
    function selectGroupByName(groupName) {
        Qt.callLater(function() {
            var idx = page.groupModel.indexOf(groupName);
            if (idx <= 0) return;
            groupCombo.currentIndex = idx;
            page.filterGroup = groupName;
            page.applyFilters();
        });
    }

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
        property string sortKey: ""
        readonly property bool sortable: sortKey !== ""
        readonly property bool activeSort: sortable && page.sortKey === sortKey
        width: w
        height: parent ? parent.height : 0
        leftPadding: 4
        rightPadding: activeSort ? 16 : 0
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
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
                text: I18n.t("Серверы")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontTitle
                font.bold: true
            }
            Text {
                visible: page.selCount > 0
                text: I18n.t("· выбрано: ") + page.selCount
                color: Theme.textMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                Layout.alignment: Qt.AlignVCenter
            }
            Item { Layout.fillWidth: true }

            // Visible, filled search field with a search glyph.
            Rectangle {
                id: searchBox
                Layout.preferredWidth: Math.min(300, Math.max(220, page.width * 0.28))
                Layout.minimumWidth: 220
                Layout.maximumWidth: 300
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
                    placeholderText: I18n.t("Поиск серверов…")
                    placeholderTextColor: Theme.textFaint
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: page.cellFont
                    onTextChanged: { page.filterText = text; page.applyFilters() }
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
                onActivated: { page.filterGroup = (currentIndex <= 0 ? "" : currentText); page.applyFilters() }
            }
            FilterCombo {
                id: tagCombo
                Layout.preferredWidth: 160
                width: 160
                model: page.tagModel
                onActivated: { page.filterTag = (currentIndex <= 0 ? "" : currentText); page.applyFilters() }
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
                tip: page.sortAsc ? I18n.t("По возрастанию") : I18n.t("По убыванию")
                onClicked: {
                    page.sortAsc = !page.sortAsc;
                    App.setNodeSort(page.sortKey, page.sortAsc);
                }
            }
        }

        // ── subscription metadata: does not push server/search toolbars ─────
        RowLayout {
            visible: false
            Layout.preferredHeight: 0
            Layout.fillWidth: true
            spacing: 8

            Item { Layout.fillWidth: true }
            Text {
                Layout.maximumWidth: Math.max(220, page.width * 0.42)
                text: {
                    var meta = page.subscriptionMeta(page.selectedSub());
                    if (!meta) return "";
                    var title = meta.profileTitle || "";
                    var profile = meta.clientProfile ? (" · " + meta.clientProfile) : "";
                    return title.length ? (title + profile) : (I18n.t("Профиль подписки") + profile);
                }
                color: Theme.textMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignRight
                verticalAlignment: Text.AlignVCenter
            }
            AccentButton {
                visible: { var meta = page.subscriptionMeta(page.selectedSub()); return meta && meta.supportUrl; }
                kind: "ghost"
                iconOnly: true
                glyph: "\uE8F2"
                text: I18n.t("Поддержка подписки")
                onClicked: {
                    var meta = page.subscriptionMeta(page.selectedSub());
                    if (meta && meta.supportUrl) App.openUrl(meta.supportUrl);
                }
            }
            AccentButton {
                visible: { var meta = page.subscriptionMeta(page.selectedSub()); return meta && meta.profileUrl; }
                kind: "ghost"
                iconOnly: true
                glyph: "\uE774"
                text: I18n.t("Страница подписки")
                onClicked: {
                    var meta = page.subscriptionMeta(page.selectedSub());
                    if (meta && meta.profileUrl) App.openUrl(meta.profileUrl);
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
                Layout.alignment: Qt.AlignBottom
                spacing: 8

                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE8B5"; text: I18n.t("Импорт из буфера"); onClicked: App.importClipboard() }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE8E5"; text: I18n.t("Импорт .conf"); onClicked: App.importNodeFile() }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE710"; text: I18n.t("Создать группу"); onClicked: groupDialog.openNew() }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE8B3"; text: I18n.t("Выбрать все"); onClicked: page.selectAll() }
                // Пинг — две кнопки как в оригинале (FIF.SEND / FIF.SYNC), способ из настроек
                AccentButton { kind: "accent"; iconOnly: true; glyph: "\uE724"; text: I18n.t("Пинг выбранных"); enabled: page.selCount > 0; onClicked: App.pingNodes(page.selectedIds()) }
                // Тест скорости — две кнопки (выбранные / все)
                AccentButton { kind: "accent"; iconOnly: true; glyph: "\uEC4A"; text: I18n.t("Тест скорости выбранных"); enabled: page.selCount > 0; onClicked: App.speedTestNodes(page.selectedIds()) }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE769"; text: I18n.t("Остановить тест"); onClicked: App.cancelSpeedTest() }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE943"; text: I18n.t("Экспорт outbound JSON"); enabled: page.selCount === 1; onClicked: App.saveOutboundJson(page.firstSelected()) }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE792"; text: I18n.t("Экспорт runtime JSON"); enabled: page.selCount === 1; onClicked: App.saveRuntimeJson(page.firstSelected()) }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE72D"; text: I18n.t("Экспорт выбранных (ссылки)"); enabled: page.selCount > 0; onClicked: App.exportNodeLinks(page.selectedIds()) }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE8C8"; text: I18n.t("Экспорт всех (ссылки)"); onClicked: App.exportNodeLinks([]) }
                AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uED14"; text: I18n.t("QR выбранного"); enabled: page.selCount === 1; onClicked: App.showNodeQr(page.firstSelected()) }
                AccentButton { kind: "danger"; iconOnly: true; glyph: "\uE74D"; text: I18n.t("Удалить выбранные"); enabled: page.selCount > 0; onClicked: App.deleteNodes(page.selectedIds()) }
            }

            // Подписки (справа)
            Item {
                id: subToolbar
                Layout.alignment: Qt.AlignBottom | Qt.AlignRight
                Layout.maximumWidth: Math.min(620, page.width * 0.58)
                Layout.preferredWidth: Math.min(620, page.width * 0.58)
                Layout.preferredHeight: Theme.controlHeight
                Layout.minimumHeight: Theme.controlHeight
                Layout.maximumHeight: Theme.controlHeight

                RowLayout {
                    id: subMetaRow
                    anchors.right: parent.right
                    anchors.bottom: subControls.top
                    anchors.bottomMargin: 8
                    width: parent.width
                    height: 28
                    spacing: 6

                    Item { Layout.fillWidth: true }
                    Text {
                        Layout.maximumWidth: Math.max(160, subToolbar.width - 96)
                        Layout.preferredHeight: 20
                        opacity: page.subscriptionMeta(page.selectedSub()) !== null ? 1 : 0
                        text: {
                            var meta = page.subscriptionMeta(page.selectedSub());
                            if (!meta) return "";
                            var title = meta.profileTitle || "";
                            var profile = meta.clientProfile ? (" · " + meta.clientProfile) : "";
                            return title.length ? (title + profile) : (I18n.t("Профиль подписки") + profile);
                        }
                        color: Theme.textMuted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        elide: Text.ElideRight
                        horizontalAlignment: Text.AlignRight
                        verticalAlignment: Text.AlignVCenter
                    }
                    AccentButton {
                        visible: { var meta = page.subscriptionMeta(page.selectedSub()); return meta && meta.supportUrl; }
                        kind: "ghost"
                        iconOnly: true
                        glyph: "\uE8F2"
                        text: I18n.t("Поддержка подписки")
                        onClicked: {
                            var meta = page.subscriptionMeta(page.selectedSub());
                            if (meta && meta.supportUrl) App.openUrl(meta.supportUrl);
                        }
                    }
                    AccentButton {
                        visible: { var meta = page.subscriptionMeta(page.selectedSub()); return meta && meta.telegramUrl; }
                        kind: "ghost"
                        iconOnly: true
                        glyph: "\uE8BD"
                        text: "Telegram"
                        onClicked: {
                            var meta = page.subscriptionMeta(page.selectedSub());
                            if (meta && meta.telegramUrl) App.openUrl(meta.telegramUrl);
                        }
                    }
                    AccentButton {
                        visible: { var meta = page.subscriptionMeta(page.selectedSub()); return meta && meta.profileUrl; }
                        kind: "ghost"
                        iconOnly: true
                        glyph: "\uE774"
                        text: I18n.t("Страница подписки")
                        onClicked: {
                            var meta = page.subscriptionMeta(page.selectedSub());
                            if (meta && meta.profileUrl) App.openUrl(meta.profileUrl);
                        }
                    }
                }
                RowLayout {
                    id: subControls
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    height: Theme.controlHeight
                    spacing: 8

                    Item { Layout.fillWidth: true }
                    AccentButton { kind: "ghost"; iconOnly: true; glyph: "\uE946"; text: I18n.t("Свойства подписки"); enabled: App.subscriptions.length > 0; onClicked: infoDialog.openInfo() }
                    AccentButton { kind: "accent"; iconOnly: true; glyph: "\uE8B5"; text: I18n.t("Импорт подписки"); onClicked: subDialog.openNew() }
                    FilterCombo {
                        id: subCombo
                        Layout.preferredWidth: Math.round(Math.max(150, Math.min(260, page.width * 0.20)))
                        enabled: App.subscriptions.length > 0
                        model: App.subscriptions.length > 0
                            ? App.subscriptions.map(function(s) { return (s.name && s.name.length ? s.name : s.url) + " (" + (s.node_count || 0) + ")"; })
                            : [I18n.t("Нет подписок")]
                    }
                    AccentButton { kind: "ghost"; iconOnly: true; glyph: "\uE72C"; text: I18n.t("Обновить подписку"); enabled: App.subscriptions.length > 0; onClicked: { var s = page.selectedSub(); if (s) App.updateSubscription(s.url) } }
                    AccentButton { kind: "ghost"; iconOnly: true; glyph: "\uE895"; text: I18n.t("Обновить все подписки"); enabled: App.subscriptions.length > 0; onClicked: App.updateAllSubscriptions() }
                    AccentButton { kind: "danger"; iconOnly: true; glyph: "\uE74D"; text: I18n.t("Удалить подписку (с серверами)"); enabled: App.subscriptions.length > 0; onClicked: { var s = page.selectedSub(); if (s) App.removeSubscription(s.url, true) } }
                }
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
                interactive: false
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
                            HeaderLabel { w: page.colName; text: I18n.t("Сервер"); sortKey: "name" }
                            HeaderLabel { w: page.colType; text: I18n.t("Тип"); sortKey: "scheme" }
                            HeaderLabel { w: page.colTransport; text: I18n.t("Транспорт"); sortKey: "transport" }
                            HeaderLabel { w: page.colAddr; text: I18n.t("Адрес") }
                            HeaderLabel { w: page.colPort; text: I18n.t("Порт") }
                            HeaderLabel { w: page.colGroup; text: I18n.t("Группа"); sortKey: "group" }
                            HeaderLabel { w: page.colTags; text: I18n.t("Теги") }
                            HeaderLabel { w: page.colPing; text: I18n.t("Пинг"); sortKey: "ping" }
                            HeaderLabel { w: page.colSpeed; text: I18n.t("Скорость"); sortKey: "speed" }
                            HeaderLabel { w: page.colStatus; text: I18n.t("Статус") }
                            HeaderLabel { w: page.colLast; text: I18n.t("Последнее"); sortKey: "last" }
                        }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.divider }
                    }

                    ListView {
                        id: list
                        width: page.tableWidth
                        height: parent.height - headerRow.height
                        model: App.nodeModel
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
                        interactive: false  // wheel/scrollbar only; no drag-to-scroll list movement
                        boundsBehavior: Flickable.StopAtBounds

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
                                listVbarTrack.flash()
                                ev.accepted = true
                            }
                        }

                        delegate: Rectangle {
                            id: nodeRow
                            required property int index
                            required property string nodeId
                            required property string name
                            required property string scheme
                            required property string transport
                            required property string server
                            required property int port
                            required property string group
                            required property var tags
                            required property int ping
                            required property real speed
                            required property bool isAlive
                            required property bool selected
                            required property bool runtimeSupported
                            required property int speedProgress
                            required property string flagSource
                            required property string flagEmoji
                            required property string flagOrient
                            required property var flagColors
                            required property string lastUsed

                            readonly property bool picked: (page.selRev >= 0) && (page.sel[nodeId] === true)

                            width: page.tableWidth
                            height: page.rowH
                            opacity: runtimeSupported ? 1.0 : 0.45
                            color: picked ? Theme.accentSoft : (hover.hovered && runtimeSupported ? Theme.cardHover : "transparent")

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
                                            width: (nodeRow.flagSource || nodeRow.flagEmoji || (nodeRow.flagColors && nodeRow.flagColors.length > 0)) ? 24 : 0
                                            height: 18
                                            visible: width > 0
                                            clip: true
                                            Image {
                                                id: flagImg
                                                anchors.fill: parent
                                                visible: false
                                                source: nodeRow.flagSource
                                                fillMode: Image.PreserveAspectFit
                                                sourceSize.width: Math.round(flagBox.width * Screen.devicePixelRatio)
                                                sourceSize.height: Math.round(flagBox.height * Screen.devicePixelRatio)
                                                smooth: true
                                                mipmap: true
                                                asynchronous: true
                                                cache: true
                                            }
                                            Rectangle {
                                                id: flagMask
                                                anchors.fill: parent
                                                radius: 3
                                                visible: false
                                                antialiasing: true
                                                layer.enabled: true
                                                layer.smooth: true
                                                layer.samples: 4
                                                layer.textureSize: Qt.size(Math.round(flagBox.width * 3), Math.round(flagBox.height * 3))
                                            }
                                            MultiEffect {
                                                anchors.fill: parent
                                                visible: !!nodeRow.flagSource
                                                source: flagImg
                                                maskEnabled: true
                                                maskSource: flagMask
                                                maskThresholdMin: 0.5
                                                maskSpreadAtMin: 0.5
                                                antialiasing: true
                                                layer.enabled: true
                                                layer.smooth: true
                                                layer.samples: 4
                                                layer.textureSize: Qt.size(Math.round(flagBox.width * 3), Math.round(flagBox.height * 3))
                                            }
                                            Text {
                                                anchors.centerIn: parent
                                                visible: !nodeRow.flagSource && !!nodeRow.flagEmoji
                                                text: nodeRow.flagEmoji
                                                font.pixelSize: 18
                                                font.family: "Segoe UI Emoji"
                                                renderType: Text.NativeRendering
                                            }
                                            Column {
                                                anchors.fill: parent
                                                visible: !nodeRow.flagSource && !nodeRow.flagEmoji && nodeRow.flagOrient === "h"
                                                Repeater {
                                                    model: (!nodeRow.flagSource && !nodeRow.flagEmoji && nodeRow.flagOrient === "h" && nodeRow.flagColors) ? nodeRow.flagColors : []
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
                                                visible: !nodeRow.flagSource && !nodeRow.flagEmoji && nodeRow.flagOrient === "v"
                                                Repeater {
                                                    model: (!nodeRow.flagSource && !nodeRow.flagEmoji && nodeRow.flagOrient === "v" && nodeRow.flagColors) ? nodeRow.flagColors : []
                                                    delegate: Rectangle {
                                                        required property string modelData
                                                        width: flagBox.width / Math.max(1, nodeRow.flagColors.length)
                                                        height: flagBox.height
                                                        color: modelData
                                                    }
                                                }
                                            }
                                            Item {
                                                anchors.fill: parent
                                                visible: !nodeRow.flagSource && !nodeRow.flagEmoji && nodeRow.flagOrient === "nordic"
                                                Rectangle { anchors.fill: parent; color: (nodeRow.flagColors && nodeRow.flagColors.length > 0) ? nodeRow.flagColors[0] : "transparent" }
                                                Rectangle {
                                                    x: 0; y: Math.round(flagBox.height * 0.36)
                                                    width: flagBox.width; height: Math.max(2, Math.round(flagBox.height * 0.28))
                                                    color: (nodeRow.flagColors && nodeRow.flagColors.length > 1) ? nodeRow.flagColors[1] : "transparent"
                                                }
                                                Rectangle {
                                                    x: Math.round(flagBox.width * 0.30); y: 0
                                                    width: Math.max(3, Math.round(flagBox.width * 0.18)); height: flagBox.height
                                                    color: (nodeRow.flagColors && nodeRow.flagColors.length > 1) ? nodeRow.flagColors[1] : "transparent"
                                                }
                                                Rectangle {
                                                    visible: nodeRow.flagColors && nodeRow.flagColors.length > 2
                                                    x: 0; y: Math.round(flagBox.height * 0.43)
                                                    width: flagBox.width; height: Math.max(1, Math.round(flagBox.height * 0.14))
                                                    color: (nodeRow.flagColors && nodeRow.flagColors.length > 2) ? nodeRow.flagColors[2] : "transparent"
                                                }
                                                Rectangle {
                                                    visible: nodeRow.flagColors && nodeRow.flagColors.length > 2
                                                    x: Math.round(flagBox.width * 0.36); y: 0
                                                    width: Math.max(1, Math.round(flagBox.width * 0.09)); height: flagBox.height
                                                    color: (nodeRow.flagColors && nodeRow.flagColors.length > 2) ? nodeRow.flagColors[2] : "transparent"
                                                }
                                            }
                                            Item {
                                                anchors.fill: parent
                                                visible: !nodeRow.flagSource && !nodeRow.flagEmoji && nodeRow.flagOrient === "cross"
                                                Rectangle { anchors.fill: parent; color: (nodeRow.flagColors && nodeRow.flagColors.length > 0) ? nodeRow.flagColors[0] : "transparent" }
                                                Rectangle {
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    width: flagBox.width; height: Math.max(3, Math.round(flagBox.height * 0.30))
                                                    color: (nodeRow.flagColors && nodeRow.flagColors.length > 1) ? nodeRow.flagColors[1] : "transparent"
                                                }
                                                Rectangle {
                                                    anchors.horizontalCenter: parent.horizontalCenter
                                                    width: Math.max(3, Math.round(flagBox.width * 0.20)); height: flagBox.height
                                                    color: (nodeRow.flagColors && nodeRow.flagColors.length > 1) ? nodeRow.flagColors[1] : "transparent"
                                                }
                                            }
                                            Rectangle {
                                                anchors.fill: parent
                                                visible: !nodeRow.flagEmoji
                                                color: "transparent"
                                                radius: 3
                                                antialiasing: true
                                                border.width: 1
                                                border.color: Qt.rgba(0, 0, 0, 0.2)
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
                                CellText { w: page.colTransport; text: nodeRow.transport }
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

                    Item {
                        id: listVbarTrack
                        parent: hflick
                        z: 50
                        width: 12
                        x: hflick.width - width - 2
                        y: headerRow.height
                        height: Math.max(0, hflick.height - headerRow.height - (hbar.visible ? 10 : 0))
                        property bool forcedVisible: false
                        readonly property real maxY: Math.max(0, list.contentHeight - list.height)
                        readonly property bool scrollable: maxY > 1
                        readonly property bool shown: scrollable

                        function flash() {
                            if (!scrollable)
                                return
                            forcedVisible = true
                            listBarHide.restart()
                        }

                        Timer {
                            id: listBarHide
                            interval: 720
                            repeat: false
                            onTriggered: listVbarTrack.forcedVisible = false
                        }

                        visible: scrollable
                        opacity: shown ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 160 : 0; easing.type: Theme.easeStandard } }
                        HoverHandler { id: listBarHover }

                        Rectangle {
                            id: listThumb
                            readonly property real ratio: list.contentHeight <= 0 ? 1 : Math.min(1, list.height / list.contentHeight)
                            width: listThumbMouse.drag.active || listBarHover.hovered ? 8 : 4
                            height: Math.max(36, listVbarTrack.height * ratio)
                            x: Math.round((listVbarTrack.width - width) / 2)
                            y: listVbarTrack.maxY <= 0 ? 0 : (listVbarTrack.height - height) * (list.contentY / listVbarTrack.maxY)
                            radius: width / 2
                            color: listThumbMouse.drag.active ? Theme.textMuted : Theme.textFaint
                            opacity: listThumbMouse.drag.active ? 0.9 : (listBarHover.hovered ? 0.68 : 0.38)
                            Behavior on width { NumberAnimation { duration: Theme.animations ? 120 : 0; easing.type: Easing.OutCubic } }

                            MouseArea {
                                id: listThumbMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                drag.target: parent
                                drag.axis: Drag.YAxis
                                drag.minimumY: 0
                                drag.maximumY: Math.max(0, listVbarTrack.height - listThumb.height)
                                preventStealing: true
                                onPressed: {
                                    listScrollAnim.stop()
                                    listVbarTrack.flash()
                                }
                                onPositionChanged: {
                                    if (!drag.active)
                                        return
                                    var denom = Math.max(1, listVbarTrack.height - listThumb.height)
                                    list.contentY = Math.max(0, Math.min(listVbarTrack.maxY, (listThumb.y / denom) * listVbarTrack.maxY))
                                    listVbarTrack.flash()
                                }
                                onReleased: listVbarTrack.flash()
                                onCanceled: listVbarTrack.flash()
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
                    text: I18n.t("Список серверов пуст")
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontNormal
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: I18n.t("Скопируйте ссылки и нажмите «Импорт»")
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
            text: I18n.t("Установить как активный")
            enabled: page.selCount === 1
            onTriggered: App.selectNode(page.menuNodeId())
        }
        Sep {}
        Ctx {
            text: I18n.t("Редактировать")
            enabled: page.selCount === 1
            onTriggered: editDialog.openFor(page.firstSelected())
        }
        Ctx {
            text: I18n.t("Копировать ссылку")
            enabled: page.selCount === 1
            onTriggered: App.copyNodeLink(page.firstSelected())
        }
        Ctx {
            text: I18n.t("Массовое редактирование ({count})", { count: page.selCount })
            enabled: page.selCount > 0
            onTriggered: bulkDialog.openFor(page.selectedIds())
        }
        Ctx {
            text: I18n.t("Экспорт выбранных — ссылки ({count})", { count: page.selCount })
            enabled: page.selCount > 0
            onTriggered: App.exportNodeLinks(page.selectedIds())
        }
        Ctx {
            text: I18n.t("Экспорт в QR-код")
            enabled: page.selCount === 1
            onTriggered: App.showNodeQr(page.firstSelected())
        }
        Sep {}
        Ctx {
            text: I18n.t("Пинг — способ из настроек ({count})", { count: page.selCount })
            enabled: page.selCount > 0
            onTriggered: App.pingNodes(page.selectedIds())
        }
        Ctx {
            text: I18n.t("Тест TCP ping ({count})", { count: page.selCount })
            enabled: page.selCount > 0
            onTriggered: App.tcpingNodes(page.selectedIds())
        }
        Ctx {
            text: I18n.t("Тест реальной задержки ({count})", { count: page.selCount })
            enabled: page.selCount > 0
            onTriggered: App.realDelayNodes(page.selectedIds())
        }
        Ctx {
            text: I18n.t("Тест скорости загрузки ({count})", { count: page.selCount })
            enabled: page.selCount > 0
            onTriggered: App.downloadSpeedNodes(page.selectedIds())
        }
        Sep {}
        Ctx {
            text: I18n.t("В начало списка")
            enabled: page.selCount === 1
            onTriggered: App.reorderNode(page.menuNodeId(), "top")
        }
        Ctx {
            text: I18n.t("В конец списка")
            enabled: page.selCount === 1
            onTriggered: App.reorderNode(page.menuNodeId(), "bottom")
        }
        Sep {}
        Ctx {
            text: I18n.t("Удалить ({count})", { count: page.selCount })
            enabled: page.selCount > 0
            onTriggered: App.deleteNodes(page.selectedIds())
        }
    }

    // ── keyboard shortcuts ───────────────────────
    readonly property bool _kbReady: page.visible
        && !searchInput.activeFocus
        && !editDialog.opened
        && !bulkDialog.opened
        && !groupDialog.opened
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

    // ── диалог создания ручной группы без подписки ──────────────
    Dialog {
        id: groupDialog
        modal: true
        dim: true
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: 420
        padding: 18
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        function openNew() {
            groupNameField.text = "";
            groupDialog.open();
            groupNameField.forceActiveFocus();
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
                text: I18n.t("Новая группа")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontStrong
                font.bold: true
            }
            Text {
                text: I18n.t("Группа будет создана отдельно от подписок. В неё можно переносить серверы через редактирование или массовое редактирование.")
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Text { text: I18n.t("Имя группы"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            TextField {
                id: groupNameField
                Layout.fillWidth: true
                implicitHeight: Theme.controlHeight
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                selectByMouse: true
                leftPadding: 10; rightPadding: 10
                placeholderText: I18n.t("Например: Мои серверы")
                placeholderTextColor: Theme.textFaint
                background: Rectangle { radius: Theme.radiusSmall; color: Theme.card; border.width: 1; border.color: groupNameField.activeFocus ? Theme.accent : Theme.borderSolid }
                Keys.onReturnPressed: createGroupButton.clicked()
                Keys.onEnterPressed: createGroupButton.clicked()
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 6
                Item { Layout.fillWidth: true }
                AccentButton { kind: "ghost"; text: I18n.t("Отмена"); onClicked: groupDialog.close() }
                AccentButton {
                    id: createGroupButton
                    kind: "accent"; text: I18n.t("Создать")
                    enabled: groupNameField.text.trim().length > 0
                    onClicked: {
                        var name = groupNameField.text.trim();
                        if (App.createManualGroup(name)) {
                            groupDialog.close();
                            page.selectGroupByName(name);
                        }
                    }
                }
            }
        }
    }

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
                text: I18n.t("Импорт подписки")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontStrong
                font.bold: true
            }
            Text {
                text: I18n.t("Вставьте ссылку на подписку. Серверы автоматически попадут в новую группу.")
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Text { text: I18n.t("URL подписки"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
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

            Text { text: I18n.t("Имя группы (необязательно)"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            TextField {
                id: subNameField
                Layout.fillWidth: true
                implicitHeight: Theme.controlHeight
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                selectByMouse: true
                leftPadding: 10; rightPadding: 10
                placeholderText: I18n.t("Пусто — имя будет создано автоматически")
                placeholderTextColor: Theme.textFaint
                background: Rectangle { radius: Theme.radiusSmall; color: Theme.card; border.width: 1; border.color: subNameField.activeFocus ? Theme.accent : Theme.borderSolid }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 6
                Item { Layout.fillWidth: true }
                AccentButton { kind: "ghost"; text: I18n.t("Отмена"); onClicked: subDialog.close() }
                AccentButton {
                    kind: "accent"; text: I18n.t("Импортировать")
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
                text: I18n.t("Свойства подписки")
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
                    : [I18n.t("Нет подписок")]
            }

            // Свойства выбранной подписки
            Column {
                Layout.fillWidth: true
                spacing: 6
                Repeater {
                    model: page.infoRows(page.infoSub())
                    delegate: RowLayout {
                        id: infoRow
                        width: parent ? parent.width : 0
                        spacing: 12
                        readonly property bool isUrlRow: String(modelData[0]).toLowerCase() === "url"
                        Text {
                            text: modelData[0]
                            color: Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            Layout.preferredWidth: 140
                        }
                        Text {
                            visible: !infoRow.isUrlRow
                            text: modelData[1]
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            wrapMode: Text.WrapAnywhere
                            Layout.fillWidth: true
                        }
                        TextEdit {
                            visible: infoRow.isUrlRow
                            text: modelData[1]
                            color: urlMouse.containsMouse ? Theme.accent : Theme.text
                            selectedTextColor: Theme.text
                            selectionColor: Theme.accentSoft
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            wrapMode: TextEdit.WrapAnywhere
                            readOnly: true
                            selectByMouse: true
                            textFormat: TextEdit.PlainText
                            Layout.fillWidth: true

                            MouseArea {
                                id: urlMouse
                                anchors.fill: parent
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                cursorShape: Qt.PointingHandCursor
                                hoverEnabled: true
                                onClicked: function(mouse) {
                                    if (mouse.button === Qt.RightButton) {
                                        subUrlMenu.url = modelData[1];
                                        subUrlMenu.popup();
                                    } else {
                                        App.openUrl(modelData[1]);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Text {
                visible: { var s = page.infoSub(); return s ? !(s.userinfo && typeof s.userinfo === "object" && Object.keys(s.userinfo).length > 0) : false; }
                text: I18n.t("В ссылке этой подписки нет доп. информации о пользователе.")
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
                AccentButton { kind: "accent"; text: I18n.t("Закрыть"); onClicked: infoDialog.close() }
            }
        }
    }

    Menu {
        id: subUrlMenu
        property string url: ""
        MenuItem {
            text: I18n.t("Копировать ссылку")
            onTriggered: App.copyText(subUrlMenu.url)
        }
    }

    // ---- QR dialog ---------------------------------------------------
    Dialog {
        id: qrDialog
        property string qrSource: ""
        property string qrName: ""
        anchors.centerIn: Overlay.overlay
        modal: true
        title: qrName.length ? I18n.t("QR-код: ") + qrName : I18n.t("QR-код сервера")
        standardButtons: Dialog.Close
        width: 360
        contentItem: ColumnLayout {
            spacing: 12
            Image {
                source: qrDialog.qrSource
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: 280
                Layout.preferredHeight: 280
                fillMode: Image.PreserveAspectFit
                smooth: false
            }
            Text {
                text: I18n.t("Отсканируйте код в клиенте, чтобы импортировать сервер.")
                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter; Layout.fillWidth: true
            }
        }
    }

    Connections {
        target: App
        function onNodeImported(nodeId) {
            page.revealImportedNode(nodeId);
        }
        function onNodeQrReady(dataUri, name) {
            qrDialog.qrSource = dataUri;
            qrDialog.qrName = name;
            qrDialog.open();
        }
    }
}
