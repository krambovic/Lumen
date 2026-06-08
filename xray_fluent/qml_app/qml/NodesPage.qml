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
        property int w: 80
        width: w
        height: parent ? parent.height : 0
        leftPadding: 4
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        color: Theme.textFaint
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.bold: true
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
        Flow {
            Layout.fillWidth: true
            spacing: 8

            AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE8B5"; text: "Импорт из буфера"; onClicked: App.importClipboard() }
            AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE8B3"; text: "Выбрать все"; onClicked: page.selectAll() }
            AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE894"; text: "Снять выбор"; enabled: page.selCount > 0; onClicked: page.clearSel() }
            AccentButton { kind: "accent"; iconOnly: true; glyph: "\uE724"; text: "Пинг выбранных"; enabled: page.selCount > 0; onClicked: App.pingNodes(page.selectedIds()) }
            AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE724"; text: "Пинг всех"; onClicked: App.pingNodes() }
            AccentButton { kind: "accent"; iconOnly: true; glyph: "\uEC4A"; text: "Тест скорости выбранных"; enabled: page.selCount > 0; onClicked: App.speedTestNodes(page.selectedIds()) }
            AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uEC4A"; text: "Тест скорости всех"; onClicked: App.speedTestNodes() }
            AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE769"; text: "Остановить тест"; onClicked: App.cancelSpeedTest() }
            AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE943"; text: "Экспорт outbound JSON"; enabled: page.selCount === 1; onClicked: App.saveOutboundJson(page.firstSelected()) }
            AccentButton { kind: "ghost";  iconOnly: true; glyph: "\uE792"; text: "Экспорт runtime JSON"; enabled: page.selCount === 1; onClicked: App.saveRuntimeJson(page.firstSelected()) }
            AccentButton { kind: "danger"; iconOnly: true; glyph: "\uE74D"; text: "Удалить выбранные"; enabled: page.selCount > 0; onClicked: App.deleteNodes(page.selectedIds()) }
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
                            HeaderLabel { w: page.colName; text: "Сервер" }
                            HeaderLabel { w: page.colType; text: "Тип" }
                            HeaderLabel { w: page.colAddr; text: "Адрес" }
                            HeaderLabel { w: page.colPort; text: "Порт" }
                            HeaderLabel { w: page.colGroup; text: "Группа" }
                            HeaderLabel { w: page.colTags; text: "Теги" }
                            HeaderLabel { w: page.colPing; text: "Пинг" }
                            HeaderLabel { w: page.colSpeed; text: "Скорость" }
                            HeaderLabel { w: page.colStatus; text: "Статус" }
                            HeaderLabel { w: page.colLast; text: "Последнее" }
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
            text: "Пинг (" + page.selCount + ")"
            enabled: page.selCount > 0
            onTriggered: App.pingNodes(page.selectedIds())
        }
        Ctx {
            text: "Тест скорости (" + page.selCount + ")"
            enabled: page.selCount > 0
            onTriggered: App.speedTestNodes(page.selectedIds())
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

    // Диалоги редактирования (одиночного и массового) — порт ui/node_edit_dialog.py и ui/bulk_edit_dialog.py.
    NodeEditDialog { id: editDialog }
    BulkEditDialog { id: bulkDialog }
}
