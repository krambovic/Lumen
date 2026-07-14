import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

// Faithful port of ui/routing_page.py, now fully wired to the bridge:
//  - Поведение / пользовательские пресеты / Обход локальной сети
//  - быстрые пресеты
//  - Приложения (process_rules) / Сервисы (service_routes) / Домены и IP
// Each control applies immediately through the App bridge (applyRoutingPreset,
// applyCustomRoutingPreset, setBypassLan, setTunDefaultOutbound,
// addProcessRule/removeProcessRule, setServiceRoute, addDomainRule/
// removeDomainRule, importDomainRules/exportDomainRules, applyRoutingPreset).
Item {
    id: page

    readonly property var builtInPresetKeys: ["global", "blocked", "except_ru"]
    function idxIn(arr, v) {
        var i = arr.indexOf(v);
        return i < 0 ? 0 : i;
    }

    readonly property var tunOutKeys: ["proxy", "direct"]
    readonly property var procActionKeys: ["direct", "proxy", "block"]
    readonly property var procActionLabels: [I18n.t("Прямой"), I18n.t("Прокси"), I18n.t("Блокировка")]
    readonly property var svcActionKeys: ["direct", "proxy"]
    readonly property var svcActionLabels: [I18n.t("Прямой"), I18n.t("Прокси")]

    // ---- reusable styled combo (Windows 11 Fluent look) --------------
    component StyledCombo: FluentCombo {}

    component SectionLabel: Text {
        color: Theme.text; font.family: Theme.fontFamily
        font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
    }

    component DnsField: TextField {
        Layout.fillWidth: true
        color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal
        selectByMouse: true
        background: Rectangle {
            radius: Theme.radiusSmall; color: Theme.card
            border.width: 1; border.color: Theme.borderSolid
        }
    }

    // Fluent-стилизованный диалог (скруглённая карточка + кастомные кнопки) вместо дефолтного Metro-окна либы.
    component FluentDialog: Dialog {
        id: fdlg
        property string okText: I18n.t("Добавить")
        anchors.centerIn: Overlay.overlay
        modal: true
        padding: 20
        topPadding: 12
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle {
            radius: 10; color: Theme.flyout
            border.width: 1; border.color: Theme.flyoutBorder
        }
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.45) }
        header: Text {
            text: fdlg.title
            visible: fdlg.title.length > 0
            color: Theme.text; font.family: Theme.fontFamily
            font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
            leftPadding: 20; rightPadding: 20; topPadding: 18; bottomPadding: 2
        }
        footer: RowLayout {
            spacing: 10
            Item { Layout.fillWidth: true }
            AccentButton { kind: "ghost"; text: I18n.t("Отмена"); onClicked: fdlg.reject(); Layout.topMargin: 6; Layout.bottomMargin: 18 }
            AccentButton { kind: "accent"; text: fdlg.okText; onClicked: fdlg.accept(); Layout.rightMargin: 20; Layout.topMargin: 6; Layout.bottomMargin: 18 }
        }
    }

    FluentScroll {
        anchors.fill: parent
        roundedClip: false

        ColumnLayout {
            width: parent.width
            spacing: Theme.spacingLarge

            // ---- title + apply row -----------------------------------
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                Text {
                    text: I18n.t("Маршрутизация")
                    color: Theme.text; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontTitle; font.weight: Font.Bold
                }
                Item { Layout.fillWidth: true }
                AccentButton { kind: "ghost"; glyph: "\uE946"; text: I18n.t("Справка"); onClicked: helpDialog.open() }
            }

            // ---- behaviour / custom presets / bypass -----------------
            Card {
                Layout.fillWidth: true
                padding: 16
                ColumnLayout {
                    width: parent.width
                    spacing: 12

                    GridLayout {
                        Layout.fillWidth: true
                        columns: page.width < 850 ? 1 : 3
                        rowSpacing: 12
                        columnSpacing: 16
                        ColumnLayout {
                            spacing: 4
                            Layout.fillWidth: true
                            Layout.preferredWidth: 1
                            Text { text: I18n.t("Быстрые пресеты маршрутизации"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            StyledCombo {
                                id: builtInPresetCombo
                                Layout.fillWidth: true
                                Layout.preferredHeight: Theme.controlHeight
                                textRole: "label"
                                model: [
                                    { key: "global", label: I18n.t("Всё через VPN") },
                                    { key: "blocked", label: I18n.t("Только заблокированное") },
                                    { key: "except_ru", label: I18n.t("Всё кроме РФ") }
                                ]
                                currentIndex: page.builtInPresetKeys.indexOf(App.activeRoutingPresetId)
                                onActivated: App.applyRoutingPreset(page.builtInPresetKeys[currentIndex])
                            }
                        }
                        ColumnLayout {
                            spacing: 4
                            Layout.fillWidth: true
                            Layout.preferredWidth: 1
                            Text { text: I18n.t("Свои пресеты маршрутизации"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            StyledCombo {
                                id: customPresetCombo
                                Layout.fillWidth: true
                                Layout.preferredHeight: Theme.controlHeight
                                textRole: "name"
                                model: App.customRoutingPresets
                                enabled: App.customRoutingPresets.length > 0
                                currentIndex: {
                                    for (var i = 0; i < App.customRoutingPresets.length; i++)
                                        if (App.customRoutingPresets[i].id === App.activeRoutingPresetId) return i;
                                    return -1;
                                }
                                onActivated: {
                                    var preset = App.customRoutingPresets[currentIndex]
                                    if (preset) App.applyCustomRoutingPreset(preset.id)
                                }
                            }
                        }
                        RowLayout {
                            spacing: 8
                            Layout.alignment: Qt.AlignBottom
                            AccentButton {
                                kind: "ghost"
                                glyph: "\uE74E"
                                text: I18n.t("Сохранить текущий пресет")
                                iconOnly: true
                                tip: I18n.t("Сохранить текущие правила как пресет")
                                onClicked: { savePresetInput.text = ""; savePresetDialog.open() }
                            }
                            AccentButton {
                                kind: "ghost"
                                glyph: "\uE74D"
                                text: I18n.t("Удалить пресет")
                                iconOnly: true
                                tip: I18n.t("Удалить выбранный пользовательский пресет")
                                enabled: App.customRoutingPresets.length > 0 && customPresetCombo.currentIndex >= 0
                                onClicked: {
                                    var preset = App.customRoutingPresets[customPresetCombo.currentIndex]
                                    if (preset) App.deleteRoutingPreset(preset.id)
                                }
                            }
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Text { text: I18n.t("Обход локальной сети"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                        Item { Layout.fillWidth: true }
                        Switch {
                            id: bypassSwitch
                            checked: App.bypassLan
                            onToggled: App.setBypassLan(checked)
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: App.tunMode
                        spacing: 4
                        Text { text: "route_exclude_address"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        DnsField {
                            Layout.fillWidth: true
                            text: App.tunRouteExcludeAddress
                            placeholderText: "192.168.0.0/16, 10.0.0.0/8"
                            onEditingFinished: App.setTunRouteExcludeAddress(text)
                        }
                    }

                    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.divider }

                    // Discord Voice — SOCKS5 без TUN (дублирует переключатель с главного экрана)
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        opacity: App.tunMode ? 0.45 : 1.0 // droute is incompatible with TUN mode
                        Text {
                            text: "\uE767"
                            font.family: "Segoe Fluent Icons"
                            color: App.discordProxy ? Theme.accent : Theme.textMuted
                            font.pixelSize: Theme.fontNormal
                        }
                        ColumnLayout {
                            spacing: 2
                            Layout.fillWidth: true
                            Text { text: "Discord Voice"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                            Text { text: App.tunMode ? I18n.t("Недоступно при включенном TUN — трафик Discord уже идет через VPN") : I18n.t("Голос и стримы Discord через SOCKS5 без TUN"); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        }
                        Switch {
                            enabled: !App.tunMode // droute must never run alongside TUN
                            checked: App.discordProxy
                            onToggled: App.setDiscordProxy(checked)
                        }
                    }
                }
            }

            // ---- Приложения -------------------------------------------
            SectionLabel { text: I18n.t("Приложения") }
            RowLayout {
                Layout.fillWidth: true
                spacing: 12
                Text { text: I18n.t("По умолчанию (TUN)"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                StyledCombo {
                    width: 200
                    textRole: "label"
                    model: [ { key: "proxy", label: I18n.t("Через прокси") }, { key: "direct", label: I18n.t("Напрямую") } ]
                    currentIndex: page.idxIn(page.tunOutKeys, App.tunDefaultOutbound)
                    onActivated: App.setTunDefaultOutbound(page.tunOutKeys[currentIndex])
                }
                Item { Layout.fillWidth: true }
            }
            // process toolbar
            Flow {
                Layout.fillWidth: true
                spacing: 8
                AccentButton { kind: "ghost"; glyph: "\uE8E5"; text: I18n.t("Добавить .exe"); onClicked: { procDialog.folderMode = false; procInput.text = ""; procDialog.open() } }
                AccentButton { kind: "ghost"; glyph: "\uE8B7"; text: I18n.t("Добавить папку"); onClicked: { procDialog.folderMode = true; procInput.text = ""; procDialog.open() } }
                AccentButton { kind: "ghost"; glyph: "\uE7C4"; text: I18n.t("Из запущенных"); onClicked: { runningProcDialog.reload(); runningProcDialog.open() } }
            }
            // process table
            Card {
                Layout.fillWidth: true
                padding: 0
                hoverable: true
                ColumnLayout {
                    width: parent.width
                    spacing: 0
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.margins: 14
                        Layout.bottomMargin: 8
                        spacing: 12
                        Text { text: I18n.t("Приложение / папка"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.fillWidth: true }
                        Text { text: I18n.t("Действие"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: 150 }
                        Item { Layout.preferredWidth: 40 }
                    }
                    Rectangle { Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; height: 1; color: Theme.divider }
                    // empty state
                    Item {
                        Layout.fillWidth: true; Layout.preferredHeight: 90
                        visible: App.processRules.length === 0
                        ColumnLayout {
                            anchors.centerIn: parent; spacing: 6
                            Text { text: I18n.t("Правила для приложений не заданы"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; Layout.alignment: Qt.AlignHCenter }
                            Text { text: I18n.t("Работают только в режиме TUN."); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.alignment: Qt.AlignHCenter }
                        }
                    }
                    // rows
                    Repeater {
                        model: App.processRules
                        delegate: ColumnLayout {
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            spacing: 0
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.leftMargin: 14; Layout.rightMargin: 14
                                Layout.topMargin: 8; Layout.bottomMargin: 8
                                spacing: 12
                                Text { text: modelData.process; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; elide: Text.ElideMiddle; Layout.fillWidth: true }
                                StyledCombo {
                                    Layout.preferredWidth: 150
                                    model: page.procActionLabels
                                    currentIndex: page.idxIn(page.procActionKeys, modelData.action)
                                    onActivated: App.addProcessRule(modelData.process, page.procActionKeys[currentIndex])
                                }
                                AccentButton { kind: "ghost"; glyph: "\uE74D"; text: ""; Layout.preferredWidth: 40; onClicked: App.removeProcessRule(modelData.process) }
                            }
                            Rectangle { visible: index < App.processRules.length - 1; Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; height: 1; color: Theme.divider }
                        }
                    }
                }
            }
            // proxy warning (only when not in TUN mode)
            Rectangle {
                Layout.fillWidth: true
                visible: !App.tunMode
                radius: Theme.radiusSmall
                color: Qt.rgba(Theme.warning.r, Theme.warning.g, Theme.warning.b, 0.12)
                implicitHeight: warnText.implicitHeight + 20
                Text {
                    id: warnText
                    anchors.fill: parent; anchors.margins: 10
                    text: I18n.t("В режиме системного прокси правила по приложениям не работают — включите TUN, чтобы перехватывать трафик приложений.")
                    color: Theme.warning; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap
                }
            }

            // ---- Сервисы ----------------------------------------------
            SectionLabel { text: I18n.t("Сервисы") }
            Card {
                Layout.fillWidth: true
                padding: 16
                ColumnLayout {
                    width: parent.width
                    spacing: 0
                    Text {
                        text: I18n.t("Быстрые переключатели для популярных сервисов (YouTube, Discord и др.). Выберите прямое подключение или прокси.")
                        color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap; Layout.fillWidth: true; Layout.bottomMargin: 10
                    }
                    Repeater {
                        model: App.serviceList
                        delegate: ColumnLayout {
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            spacing: 0
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 6; Layout.bottomMargin: 6
                                spacing: 12
                                ColumnLayout {
                                    Layout.fillWidth: true; spacing: 2
                                    Text { text: modelData.name; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                                    Text { visible: !!modelData.description; text: I18n.t(modelData.description); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                                }
                                StyledCombo {
                                    Layout.preferredWidth: 150
                                    model: page.svcActionLabels
                                    currentIndex: page.idxIn(page.svcActionKeys, modelData.action)
                                    onActivated: App.setServiceRoute(modelData.id, page.svcActionKeys[currentIndex])
                                }
                            }
                            Rectangle { visible: index < App.serviceList.length - 1; Layout.fillWidth: true; height: 1; color: Theme.divider }
                        }
                    }
                    Text {
                        visible: App.serviceList.length === 0
                        text: I18n.t("Список сервисов пуст."); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                    }
                }
            }

            // ---- Домены и IP ------------------------------------------
            SectionLabel { text: I18n.t("Домены и IP") }
            Flow {
                Layout.fillWidth: true
                spacing: 8
                AccentButton { kind: "ghost"; glyph: "\uE710"; text: I18n.t("Добавить"); onClicked: { domainInput.text = ""; domainDialog.open() } }
                AccentButton { kind: "ghost"; glyph: "\uE896"; text: I18n.t("Импорт"); onClicked: { importInput.text = ""; importDialog.open() } }
                AccentButton { kind: "ghost"; glyph: "\uE8E5"; text: I18n.t("Из файла"); onClicked: App.importDomainRulesFile() }
                AccentButton { kind: "ghost"; glyph: "\uE72D"; text: I18n.t("Экспорт"); onClicked: App.exportDomainRules() }
            }
            Card {
                Layout.fillWidth: true
                padding: 0
                hoverable: true
                ColumnLayout {
                    width: parent.width
                    spacing: 0
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.margins: 14
                        Layout.bottomMargin: 8
                        spacing: 12
                        Text { text: I18n.t("Адрес"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.fillWidth: true }
                        Text { text: I18n.t("Действие"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.weight: Font.DemiBold; Layout.preferredWidth: 150 }
                        Item { Layout.preferredWidth: 40 }
                    }
                    Rectangle { Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; height: 1; color: Theme.divider }
                    Item {
                        Layout.fillWidth: true; Layout.preferredHeight: 90
                        visible: App.domainRules.length === 0
                        ColumnLayout {
                            anchors.centerIn: parent; spacing: 6
                            Text { text: I18n.t("Правила по доменам и IP не заданы"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; Layout.alignment: Qt.AlignHCenter }
                            Text { text: I18n.t("Формат импорта: адрес|действие"); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.alignment: Qt.AlignHCenter }
                        }
                    }
                    Repeater {
                        model: App.domainRules
                        delegate: ColumnLayout {
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            spacing: 0
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.leftMargin: 14; Layout.rightMargin: 14
                                Layout.topMargin: 8; Layout.bottomMargin: 8
                                spacing: 12
                                Text { text: modelData.addr; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; elide: Text.ElideMiddle; Layout.fillWidth: true }
                                StyledCombo {
                                    Layout.preferredWidth: 150
                                    model: page.procActionLabels
                                    currentIndex: page.idxIn(page.procActionKeys, modelData.action)
                                    onActivated: App.addDomainRule(modelData.addr, page.procActionKeys[currentIndex])
                                }
                                AccentButton { kind: "ghost"; glyph: "\uE74D"; text: ""; Layout.preferredWidth: 40; onClicked: App.removeDomainRule(modelData.addr) }
                            }
                            Rectangle { visible: index < App.domainRules.length - 1; Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; height: 1; color: Theme.divider }
                        }
                    }
                }
            }

            Item { Layout.preferredHeight: 4 }
        }
    }

    // ---- help dialog (Fluent-стилизованный, как ContentDialog WinUI) --
    Dialog {
        id: helpDialog
        anchors.centerIn: Overlay.overlay
        modal: true
        width: 460
        padding: 0
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        // Скруглённая карточка с hairline-границей вместо квадратного Metro-окна.
        background: Rectangle {
            radius: 10
            color: Theme.flyout
            border.width: 1
            border.color: Theme.flyoutBorder
        }

        // Мягкое затемнение фона вместо дефолтного.
        Overlay.modal: Rectangle {
            color: Qt.rgba(0, 0, 0, 0.45)
        }

        header: Text {
            text: I18n.t("Как работает маршрутизация")
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
                Layout.fillWidth: true
                text: I18n.t("Быстрые пресеты:\n«Всё через VPN» отправляет весь трафик через сервер.\n«Только заблокированное» направляет через прокси известные заблокированные ресурсы.\n«Всё кроме РФ» пускает российские ресурсы напрямую, а остальной трафик — через прокси.\n\nСервисы:\n«Прокси» — всегда вести сервис через VPN/прокси.\n«Прямой» — всегда вести сервис напрямую.\n\nПравила по приложениям работают только в режиме TUN.")
                color: Theme.textMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                wrapMode: Text.WordWrap
                lineHeight: 1.25
                leftPadding: 20; rightPadding: 20; bottomPadding: 6
            }
        }

        footer: RowLayout {
            spacing: 0
            Item { Layout.fillWidth: true }
            AccentButton {
                kind: "accent"
                text: I18n.t("Понятно")
                onClicked: helpDialog.close()
                Layout.rightMargin: 20
                Layout.topMargin: 6
                Layout.bottomMargin: 18
            }
        }
    }

    // ---- save routing preset dialog ----------------------------------
    Dialog {
        id: savePresetDialog
        anchors.centerIn: Overlay.overlay
        modal: true
        title: I18n.t("Сохранить пресет маршрутизации")
        standardButtons: Dialog.Ok | Dialog.Cancel
        width: 460
        onAccepted: { if (savePresetInput.text.trim().length) App.saveRoutingPreset(savePresetInput.text.trim()) }
        contentItem: ColumnLayout {
            spacing: 10
            Text {
                text: I18n.t("Текущие правила маршрутизации будут сохранены как именованный пресет.")
                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap; Layout.fillWidth: true
            }
            DnsField { id: savePresetInput; placeholderText: I18n.t("Название пресета") }
        }
    }

    // ---- running apps picker -----------------------------------------
    Dialog {
        id: runningProcDialog
        property var procList: []
        function reload() { runningProcDialog.procList = App.runningProcesses(); runProcCombo.currentIndex = 0 }
        anchors.centerIn: Overlay.overlay
        modal: true
        title: I18n.t("Запущенные приложения")
        standardButtons: Dialog.Ok | Dialog.Cancel
        width: 480
        onAccepted: {
            var name = runningProcDialog.procList[runProcCombo.currentIndex];
            if (name) App.addProcessRule(name, page.procActionKeys[runProcActionCombo.currentIndex]);
        }
        contentItem: ColumnLayout {
            spacing: 10
            Text {
                text: I18n.t("Выберите запущенное приложение и действие маршрутизации.")
                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap; Layout.fillWidth: true
            }
            StyledCombo {
                id: runProcCombo
                Layout.fillWidth: true
                model: runningProcDialog.procList
            }
            RowLayout {
                Layout.fillWidth: true; spacing: 10
                Text { text: I18n.t("Действие"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                StyledCombo {
                    id: runProcActionCombo
                    Layout.preferredWidth: 160
                    model: page.procActionLabels
                    currentIndex: 1
                }
            }
        }
    }

    // ---- add process dialog ------------------------------------------
    FluentDialog {
        id: procDialog
        property bool folderMode: false
        title: folderMode ? I18n.t("Добавить папку") : I18n.t("Добавить приложение")
        width: 480
        onAccepted: {
            var v = procInput.text.trim();
            if (v.length)
                App.addProcessRule(v, page.procActionKeys[procActionCombo.currentIndex]);
        }
        contentItem: ColumnLayout {
            spacing: 10
            Text {
                text: procDialog.folderMode
                    ? I18n.t("Укажите путь к папке. Все процессы внутри будут маршрутизироваться по выбранному действию.")
                    : I18n.t("Укажите имя процесса (например, chrome.exe) или полный путь к .exe.")
                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap; Layout.fillWidth: true
            }
            DnsField {
                id: procInput
                placeholderText: procDialog.folderMode ? "C:\\Games\\" : "chrome.exe"
            }
            RowLayout {
                Layout.fillWidth: true; spacing: 10
                Text { text: I18n.t("Действие"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                StyledCombo {
                    id: procActionCombo
                    Layout.preferredWidth: 160
                    model: page.procActionLabels
                    currentIndex: 1
                }
            }
        }
    }

    // ---- add domain dialog -------------------------------------------
    FluentDialog {
        id: domainDialog
        title: I18n.t("Добавить домен или IP")
        width: 480
        onAccepted: {
            var v = domainInput.text.trim();
            if (v.length)
                App.addDomainRule(v, page.procActionKeys[domainActionCombo.currentIndex]);
        }
        contentItem: ColumnLayout {
            spacing: 10
            Text {
                text: I18n.t("Домен (example.com), маска (*.example.com), IP или CIDR (10.0.0.0/8).")
                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap; Layout.fillWidth: true
            }
            DnsField { id: domainInput; placeholderText: "example.com" }
            RowLayout {
                Layout.fillWidth: true; spacing: 10
                Text { text: I18n.t("Действие"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                StyledCombo {
                    id: domainActionCombo
                    Layout.preferredWidth: 160
                    model: page.procActionLabels
                    currentIndex: 1
                }
            }
        }
    }

    // ---- import dialog -----------------------------------------------
    FluentDialog {
        id: importDialog
        okText: I18n.t("Импорт")
        title: I18n.t("Импорт правил")
        width: 520
        onAccepted: { if (importInput.text.trim().length) App.importDomainRules(importInput.text) }
        contentItem: ColumnLayout {
            spacing: 10
            Text {
                text: I18n.t("Одно правило в строке, формат: адрес|действие (direct/proxy/block). Без «|действие» применяется proxy.")
                color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap; Layout.fillWidth: true
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: 180
                TextArea {
                    id: importInput
                    wrapMode: TextArea.NoWrap
                    color: Theme.text; font.family: "Cascadia Mono, Consolas, monospace"; font.pixelSize: Theme.fontSmall
                    placeholderText: "example.com|proxy\nads.example.net|block\n10.0.0.0/8|direct"
                    background: Rectangle { radius: Theme.radiusSmall; color: Theme.card; border.width: 1; border.color: Theme.borderSolid }
                }
            }
        }
    }
}
