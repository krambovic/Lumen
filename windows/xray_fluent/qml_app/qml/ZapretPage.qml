import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

// page:
//   editing == false -> preset list (info card, toolbar, run-state, table)
//   editing == true  -> preset editor (name/description/content + save)
Item {
    id: page

    readonly property var savedTableLayout: App.zapretTableLayout || ({})
    property bool manualColumnWidths: savedTableLayout.manual === true
    property real manualColName: savedTableLayout.name !== undefined ? Number(savedTableLayout.name) : 200
    property real manualColDescription: savedTableLayout.description !== undefined ? Number(savedTableLayout.description) : 300
    property real manualColArgs: savedTableLayout.args !== undefined ? Number(savedTableLayout.args) : 110
    property real manualColDate: savedTableLayout.date !== undefined ? Number(savedTableLayout.date) : 150
    readonly property int colName: Math.round(manualColName)
    readonly property int colArgs: Math.round(manualColArgs)
    readonly property int colDate: Math.round(manualColDate)
    readonly property int colDescription: Math.round(manualColumnWidths
        ? manualColDescription
        : Math.max(120, tableViewport.width - 64 - colName - colArgs - colDate))
    readonly property int tableWidth: manualColumnWidths
        ? 64 + colName + colDescription + colArgs + colDate
        : tableViewport.width

    property var presets: []
    property string selected: ""
    property bool running: false
    property string activePreset: ""

    function freezeColumnWidths() {
        if (manualColumnWidths)
            return
        manualColName = colName
        manualColDescription = colDescription
        manualColArgs = colArgs
        manualColDate = colDate
        manualColumnWidths = true
    }

    function resizeColumn(index, width) {
        freezeColumnWidths()
        var minWidth = index === 0 ? 90 : (index === 1 ? 120 : 64)
        var value = Math.max(minWidth, Math.min(2000, Math.round(width)))
        if (index === 0) manualColName = value
        else if (index === 1) manualColDescription = value
        else if (index === 2) manualColArgs = value
        else if (index === 3) manualColDate = value
        persistTableLayout()
    }

    function persistTableLayout() {
        App.setZapretTableLayout({
            "manual": manualColumnWidths,
            "name": manualColName,
            "description": manualColDescription,
            "args": manualColArgs,
            "date": manualColDate
        })
    }

    component ColumnGuide: Rectangle {
        width: 1
        height: 40
        x: parent.width + 5
        y: (parent.height - height) / 2
        color: Theme.divider
        opacity: 0.5
    }

    component ResizableHeader: Item {
        id: headerCell
        property string label: ""
        property int columnIndex: -1
        implicitHeight: 24
        Text {
            anchors.left: parent.left
            anchors.right: resizeHandle.left
            anchors.leftMargin: 4
            anchors.rightMargin: 3
            anchors.verticalCenter: parent.verticalCenter
            text: headerCell.label
            elide: Text.ElideRight
            color: Theme.textMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
        }
        ColumnGuide { visible: headerCell.columnIndex < 3 }
        MouseArea {
            id: resizeHandle
            z: 2
            width: 10
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: parent.right
            acceptedButtons: Qt.LeftButton
            cursorShape: Qt.SplitHCursor
            property real pressedX: 0
            property real pressedWidth: 0
            onPressed: (mouse) => {
                var point = mapToItem(page, mouse.x, mouse.y)
                pressedX = point.x
                pressedWidth = headerCell.width
                mouse.accepted = true
            }
            onPositionChanged: (mouse) => {
                if (!pressed)
                    return
                var point = mapToItem(page, mouse.x, mouse.y)
                page.resizeColumn(headerCell.columnIndex, pressedWidth + point.x - pressedX)
            }
        }
    }

    // editor state
    property bool editing: false
    property bool editIsNew: false
    property string editOrigName: ""

    // Fluent-поле ввода (как в остальном приложении) вместо дефолтного TextField либы.
    component FluentField: FluentTextField {
        Layout.fillWidth: true
        implicitHeight: Theme.controlHeight
        leftPadding: 10; rightPadding: 10; topPadding: 0; bottomPadding: 0
        color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal
        selectByMouse: true
        verticalAlignment: TextInput.AlignVCenter
        background: Rectangle {
            radius: Theme.radiusSmall; color: Theme.controlFill
            border.width: 1; border.color: parent.activeFocus ? Theme.accent : Theme.borderSolid
        }
    }

    function refresh() {
        presets = App.zapretPresets()
        // keep selection valid
        var stillThere = false
        for (var i = 0; i < presets.length; ++i)
            if (presets[i].name === selected) { stillThere = true; break }
        if (!stillThere) selected = presets.length > 0 ? presets[0].name : ""
    }

    function syncStatus() {
        var s = App.zapretStatus()
        running = !!s.running
        activePreset = s.preset || ""
    }

    function openEditor(name) {
        editIsNew = (name === "")
        editOrigName = name
        nameField.text = name
        if (name === "") {
            descField.text = ""
            contentArea.text = ""
            metaText.text = ""
        } else {
            var info = null
            for (var i = 0; i < presets.length; ++i)
                if (presets[i].name === name) { info = presets[i]; break }
            descField.text = info ? info.description : ""
            contentArea.text = App.readPreset(name)
            var parts = []
            if (info && info.created) parts.push(I18n.t("Создан: ") + info.created)
            if (info && info.modified) parts.push(I18n.t("Изменён: ") + info.modified)
            metaText.text = parts.join("   |   ")
        }
        editing = true
    }

    function saveEditor() {
        var nm = nameField.text.trim()
        if (nm === "") return
        App.savePreset(nm, descField.text.trim(), contentArea.text)
        editing = false
        selected = nm
    }

    Component.onCompleted: { refresh(); syncStatus() }

    Connections {
        target: App
        function onZapretState(s) {
            page.running = !!s.running
            page.activePreset = s.preset || ""
        }
        function onZapretPresetsChanged() { page.refresh() }
        function onZapretTableLayoutChanged() {
            page.manualColumnWidths = App.zapretTableLayout.manual === true
            page.manualColName = Number(App.zapretTableLayout.name || 200)
            page.manualColDescription = Number(App.zapretTableLayout.description || 300)
            page.manualColArgs = Number(App.zapretTableLayout.args || 110)
            page.manualColDate = Number(App.zapretTableLayout.date || 150)
        }
    }

    // ════════════════ LIST VIEW ════════════════
    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacingLarge
        visible: !page.editing

        RowLayout {
            Layout.fillWidth: true
            Text {
                text: I18n.t("Обход блокировок (zapret)")
                color: Theme.text; font.family: Theme.fontFamily
                font.pixelSize: Theme.fontTitle; font.weight: Font.DemiBold
            }
            Item { Layout.fillWidth: true }
            Rectangle {
                radius: 12; implicitHeight: 24
                implicitWidth: stateRow.implicitWidth + 22
                color: page.running ? Qt.rgba(Theme.success.r, Theme.success.g, Theme.success.b, 0.16)
                                    : Qt.rgba(Theme.textMuted.r, Theme.textMuted.g, Theme.textMuted.b, 0.12)
                RowLayout {
                    id: stateRow
                    anchors.centerIn: parent
                    spacing: 6
                    Rectangle { width: 8; height: 8; radius: 4; color: page.running ? Theme.success : Theme.textMuted }
                    Text {
                        text: page.running ? (I18n.t("Работает: ") + page.activePreset) : I18n.t("Остановлен")
                        color: page.running ? Theme.success : Theme.textMuted
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                    }
                }
            }
        }

        Card {
            Layout.fillWidth: true
            padding: 16
            hoverable: false
            ColumnLayout {
                width: parent.width
                spacing: 6
                Text { text: I18n.t("Обход DPI-блокировок"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }
                Text {
                    text: I18n.t("Zapret запускает winws2.exe с выбранным пресетом аргументов, чтобы обходить DPI-замедления YouTube, Discord и других сервисов. Работает независимо от VPN/прокси и требует прав администратора.")
                    color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                    wrapMode: Text.WordWrap; Layout.fillWidth: true
                }
            }
        }

        // ---- toolbar ----
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            AccentButton { kind: "ghost"; glyph: "\uE710"; text: I18n.t("Добавить"); onClicked: page.openEditor("") }
            AccentButton { kind: "ghost"; glyph: "\uE8B5"; text: I18n.t("Импорт из файла"); onClicked: { var n = App.importZapretPreset(); if (n) page.selected = n } }
            AccentButton { kind: "ghost"; glyph: "\uE74D"; text: I18n.t("Удалить"); enabled: page.selected !== ""; onClicked: if (page.selected !== "") App.deletePreset(page.selected) }
            AccentButton { kind: "ghost"; glyph: "\uE72C"; text: I18n.t("Обновить"); onClicked: { page.refresh(); page.syncStatus() } }
            Item { Layout.fillWidth: true }
            AccentButton { kind: "accent"; glyph: "\uE768"; text: page.running ? I18n.t("Перезапустить") : I18n.t("Запустить"); enabled: page.selected !== ""; onClicked: if (page.selected !== "") App.startZapret(page.selected) }
            AccentButton { kind: "danger"; glyph: "\uE71A"; text: I18n.t("Остановить"); enabled: page.running; onClicked: App.stopZapret() }
        }

        // ---- presets table ----
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            padding: 0
            hoverable: false
            Flickable {
                id: tableViewport
                anchors.fill: parent
                contentWidth: page.tableWidth
                contentHeight: height
                flickableDirection: Flickable.HorizontalFlick
                interactive: false
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.horizontal: FluentScrollBar {
                    parent: tableViewport
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    interactive: true
                    policy: page.tableWidth > tableViewport.width ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
                }

                ColumnLayout {
                    width: page.tableWidth
                    height: tableViewport.height
                    spacing: 0
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: 14
                        Layout.rightMargin: 14
                        Layout.topMargin: 8
                        Layout.bottomMargin: 8
                        spacing: 12
                        ResizableHeader { label: I18n.t("Имя"); columnIndex: 0; Layout.preferredWidth: page.colName }
                        ResizableHeader { label: I18n.t("Описание"); columnIndex: 1; Layout.preferredWidth: page.colDescription; Layout.fillWidth: !page.manualColumnWidths }
                        ResizableHeader { label: I18n.t("Аргументов"); columnIndex: 2; Layout.preferredWidth: page.colArgs }
                        ResizableHeader { label: I18n.t("Изменён"); columnIndex: 3; Layout.preferredWidth: page.colDate }
                    }
                    Rectangle { Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; height: 1; color: Theme.divider }

                    ListView {
                        id: list
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        model: page.presets
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: FluentScrollBar { id: zapretListVbar }

                    NumberAnimation {
                        id: zapretListScrollAnim
                        target: list
                        property: "contentY"
                        duration: 540
                        easing.type: Easing.OutQuint
                    }
                    WheelHandler {
                        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                        onWheel: (ev) => {
                            var maxY = Math.max(0, list.contentHeight - list.height)
                            if (maxY <= 0) { ev.accepted = true; return }
                            if (ev.pixelDelta.y !== 0) {
                                zapretListScrollAnim.stop()
                                list.contentY = Math.max(0, Math.min(maxY, list.contentY - ev.pixelDelta.y))
                            } else {
                                var step = 92
                                var base = zapretListScrollAnim.running ? zapretListScrollAnim.to : list.contentY
                                zapretListScrollAnim.to = Math.max(0, Math.min(maxY, base - (ev.angleDelta.y / 120) * step))
                                zapretListScrollAnim.restart()
                            }
                            zapretListVbar.flash()
                            ev.accepted = true
                        }
                    }

                    delegate: Rectangle {
                        width: ListView.view.width
                        height: 40
                        readonly property bool isActive: page.running && modelData.name === page.activePreset
                        readonly property bool isSel: modelData.name === page.selected
                        radius: Theme.radiusSmall
                        color: isSel ? Theme.cardHover : (rowMouse.containsMouse ? Theme.controlFill : "transparent")
                        border.width: isSel || rowMouse.containsMouse ? 1 : 0
                        border.color: Theme.borderSolid

                        MouseArea {
                            id: rowMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: page.selected = modelData.name
                            onDoubleClicked: page.openEditor(modelData.name)
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 14
                            anchors.rightMargin: 14
                            spacing: 12
                            Text {
                                text: modelData.name; elide: Text.ElideRight
                                color: parent.parent.isActive ? Theme.success : Theme.text
                                font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal
                                font.weight: parent.parent.isActive ? Font.DemiBold : Font.Normal
                                Layout.preferredWidth: page.colName
                                ColumnGuide {}
                            }
                            Text {
                                text: modelData.description; elide: Text.ElideRight
                                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                                Layout.preferredWidth: page.colDescription
                                Layout.fillWidth: !page.manualColumnWidths
                                ColumnGuide {}
                            }
                            Text {
                                text: "" + modelData.argCount
                                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                                horizontalAlignment: Text.AlignRight; Layout.preferredWidth: page.colArgs
                                ColumnGuide {}
                            }
                            Text {
                                text: modelData.modified
                                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                                horizontalAlignment: Text.AlignRight; Layout.preferredWidth: page.colDate
                            }
                        }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.divider; opacity: 0.5 }
                    }

                    // empty state
                    Item {
                        anchors.fill: parent
                        visible: page.presets.length === 0
                        ColumnLayout {
                            anchors.centerIn: parent
                            spacing: 8
                            Text { text: "\uE945"; font.family: "Segoe Fluent Icons"; font.pixelSize: 30; color: Theme.textFaint; Layout.alignment: Qt.AlignHCenter }
                            Text { text: I18n.t("Нет пресетов Zapret"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; Layout.alignment: Qt.AlignHCenter }
                            Text { text: I18n.t("Создайте пресет или импортируйте файл аргументов winws2."); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.alignment: Qt.AlignHCenter }
                        }
                    }
                }
            }
        }
    }
    }

    // ════════════════ EDITOR VIEW ════════════════
    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacing
        visible: page.editing

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            AccentButton { kind: "ghost"; glyph: "\uE72B"; iconOnly: true; tip: I18n.t("Назад к списку"); onClicked: page.editing = false }
            Text {
                text: page.editIsNew ? I18n.t("Новый пресет") : (I18n.t("Редактирование: ") + page.editOrigName)
                color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontTitle; font.weight: Font.DemiBold
            }
            Item { Layout.fillWidth: true }
            AccentButton { kind: "accent"; glyph: "\uE792"; text: I18n.t("Сохранить"); onClicked: page.saveEditor() }
        }

        // metadata card
        Card {
            Layout.fillWidth: true
            padding: 14
            hoverable: false
            ColumnLayout {
                width: parent.width
                spacing: 10
                RowLayout {
                    Layout.fillWidth: true; spacing: 10
                    Text { text: I18n.t("Название:"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; Layout.preferredWidth: 90 }
                    FluentField {
                        id: nameField
                        Layout.fillWidth: true
                        placeholderText: I18n.t("Имя пресета")
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal
                    }
                }
                RowLayout {
                    Layout.fillWidth: true; spacing: 10
                    Text { text: I18n.t("Описание:"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; Layout.preferredWidth: 90 }
                    FluentField {
                        id: descField
                        Layout.fillWidth: true
                        placeholderText: I18n.t("Краткое описание (необязательно)")
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal
                    }
                }
                Text {
                    id: metaText
                    text: ""
                    visible: text !== ""
                    color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                }
            }
        }

        // content editor
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            padding: 2
            hoverable: false
            Flickable {
                id: editFlick
                anchors.fill: parent
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                flickDeceleration: 5000
                maximumFlickVelocity: 3000
                pixelAligned: true
                ScrollBar.vertical: FluentScrollBar { id: zapretEditorVbar }
                ScrollBar.horizontal: FluentScrollBar {}
                property real wheelStep: 92
                NumberAnimation {
                    id: editScrollAnim
                    target: editFlick; property: "contentY"
                    duration: 320; easing.type: Easing.OutQuint
                }
                WheelHandler {
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                    onWheel: (ev) => {
                        var maxY = Math.max(0, editFlick.contentHeight - editFlick.height)
                        if (maxY <= 0) { ev.accepted = true; return }
                        if (ev.pixelDelta.y !== 0) {
                            editScrollAnim.stop()
                            editFlick.contentY = Math.max(0, Math.min(maxY, editFlick.contentY - ev.pixelDelta.y))
                        } else {
                            var base = editScrollAnim.running ? editScrollAnim.to : editFlick.contentY
                            var target = Math.max(0, Math.min(maxY, base - (ev.angleDelta.y / 120) * editFlick.wheelStep))
                            editScrollAnim.to = target
                            editScrollAnim.restart()
                        }
                        zapretEditorVbar.flash()
                        ev.accepted = true
                    }
                }
                TextArea.flickable: FluentTextArea {
                    id: contentArea
                    placeholderText: I18n.t("Аргументы winws2, по одному на строку.\nСтроки с # — комментарии.")
                    wrapMode: TextEdit.NoWrap
                    font.family: "Consolas"; font.pixelSize: 13
                    color: Theme.text
                    selectByMouse: true
                    background: Rectangle { color: Theme.micaBase; radius: Theme.radiusSmall }
                }
            }
        }
    }
}
