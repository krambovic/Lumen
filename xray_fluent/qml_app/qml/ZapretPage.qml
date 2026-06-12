import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

// Live port of ui/zapret_page.py + preset_edit_widget.py. Two views inside one
// page (mirroring the classic QStackedWidget):
//   editing == false -> preset list (info card, toolbar, run-state, table)
//   editing == true  -> preset editor (name/description/content + save)
//
// All data + actions go through the bridge: App.zapretPresets(), startZapret /
// stopZapret, readPreset / savePreset / deletePreset / importZapretPreset.
// Run-state and errors arrive via the zapretState signal; the list refreshes on
// zapretPresetsChanged. The real winws2 process stays in controller.zapret.
Item {
    id: page

    readonly property int colName: 200
    readonly property int colArgs: 110
    readonly property int colDate: 150

    property var presets: []
    property string selected: ""
    property bool running: false
    property string activePreset: ""

    // editor state
    property bool editing: false
    property bool editIsNew: false
    property string editOrigName: ""

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
            // Run-state pill
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

        // progress bar while running
        ProgressBar {
            Layout.fillWidth: true
            visible: page.running
            indeterminate: true
        }

        // ---- presets table ----
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            padding: 0
            hoverable: false
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                // header
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 14
                    Layout.bottomMargin: 8
                    spacing: 12
                    Text { text: I18n.t("Имя"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: page.colName }
                    Text { text: I18n.t("Описание"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.fillWidth: true }
                    Text { text: I18n.t("Аргументов"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: page.colArgs; horizontalAlignment: Text.AlignRight }
                    Text { text: I18n.t("Изменён"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: page.colDate; horizontalAlignment: Text.AlignRight }
                }
                Rectangle { Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; height: 1; color: Theme.divider }

                // rows
                ListView {
                    id: list
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: page.presets
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: FluentScrollBar {}

                    delegate: Rectangle {
                        width: ListView.view.width
                        height: 40
                        readonly property bool isActive: page.running && modelData.name === page.activePreset
                        readonly property bool isSel: modelData.name === page.selected
                        color: isSel ? Theme.cardHover : (rowHover.hovered ? Theme.controlFill : "transparent")

                        HoverHandler { id: rowHover }
                        MouseArea {
                            anchors.fill: parent
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
                            }
                            Text {
                                text: modelData.description; elide: Text.ElideRight
                                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                                Layout.fillWidth: true
                            }
                            Text {
                                text: "" + modelData.argCount
                                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                                horizontalAlignment: Text.AlignRight; Layout.preferredWidth: page.colArgs
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
                    TextField {
                        id: nameField
                        Layout.fillWidth: true
                        placeholderText: I18n.t("Имя пресета")
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal
                    }
                }
                RowLayout {
                    Layout.fillWidth: true; spacing: 10
                    Text { text: I18n.t("Описание:"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; Layout.preferredWidth: 90 }
                    TextField {
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
            ScrollView {
                anchors.fill: parent
                clip: true
                TextArea {
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
