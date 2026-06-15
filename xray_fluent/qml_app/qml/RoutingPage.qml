import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

// Faithful port of ui/routing_page.py, now fully wired to the bridge:
//  - Поведение / DNS / Обход локальной сети
//  - TUN-only DNS (bootstrap + proxy)
//  - быстрые пресеты
//  - Приложения (process_rules) / Сервисы (service_routes) / Домены и IP
// Each control applies immediately through the App bridge (setRoutingMode,
// setDnsMode, setBypassLan, setBootstrapDns/setProxyDns, setTunDefaultOutbound,
// addProcessRule/removeProcessRule, setServiceRoute, addDomainRule/
// removeDomainRule, importDomainRules/exportDomainRules, applyRoutingPreset).
Item {
    id: page

    readonly property var modeKeys: ["global", "rule", "direct"]
    function modeIndex(m) {
        var i = page.modeKeys.indexOf(m);
        return i < 0 ? 1 : i;
    }
    function idxIn(arr, v) {
        var i = arr.indexOf(v);
        return i < 0 ? 0 : i;
    }

    readonly property var dnsModeKeys: ["system", "builtin"]
    readonly property var dnsTypes: ["udp", "tcp", "tls", "https"]
    readonly property var tunOutKeys: ["proxy", "direct"]
    readonly property var procActionKeys: ["direct", "proxy", "block"]
    readonly property var procActionLabels: [I18n.t("Прямой"), I18n.t("Прокси"), I18n.t("Блокировка")]
    readonly property var svcActionKeys: ["off", "proxy", "direct"]
    readonly property var svcActionLabels: [I18n.t("По пресету"), I18n.t("Прокси"), I18n.t("Прямой")]

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

            // ---- behaviour / DNS / bypass ----------------------------
            Card {
                Layout.fillWidth: true
                padding: 16
                ColumnLayout {
                    width: parent.width
                    spacing: 12

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 16
                        ColumnLayout {
                            spacing: 4
                            Layout.fillWidth: true
                            Text { text: I18n.t("Поведение"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            StyledCombo {
                                id: modeCombo
                                Layout.fillWidth: true
                                textRole: "label"
                                model: [
                                    { key: "global", label: I18n.t("Всё через VPN") },
                                    { key: "rule",   label: I18n.t("По моим правилам") },
                                    { key: "direct", label: I18n.t("Без VPN по умолчанию") }
                                ]
                                currentIndex: page.modeIndex(App.routingMode)
                                onActivated: App.setRoutingMode(page.modeKeys[currentIndex])
                            }
                        }
                        ColumnLayout {
                            spacing: 4
                            Layout.fillWidth: true
                            Text { text: "DNS"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            StyledCombo {
                                Layout.fillWidth: true
                                textRole: "label"
                                model: [ { key: "system", label: I18n.t("Системный") }, { key: "builtin", label: I18n.t("Встроенный") } ]
                                currentIndex: page.idxIn(page.dnsModeKeys, App.dnsMode)
                                onActivated: App.setDnsMode(page.dnsModeKeys[currentIndex])
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

                    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.divider }

                    // Discord voice — SOCKS5 без TUN (дублирует переключатель с главного экрана)
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Text {
                            text: "\uE767"
                            font.family: "Segoe Fluent Icons"
                            color: App.discordProxy ? Theme.accent : Theme.textMuted
                            font.pixelSize: Theme.fontNormal
                        }
                        ColumnLayout {
                            spacing: 2
                            Layout.fillWidth: true
                            Text { text: "Discord voice"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
                            Text { text: I18n.t("Голос и стримы Discord через SOCKS5 без TUN"); color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        }
                        Switch {
                            checked: App.discordProxy
                            onToggled: App.setDiscordProxy(checked)
                        }
                    }
                }
            }

            // ---- TUN-only DNS block ----------------------------------
            Card {
                Layout.fillWidth: true
                visible: App.tunMode
                padding: 16
                ColumnLayout {
                    width: parent.width
                    spacing: 10
                    SectionLabel { text: I18n.t("DNS для TUN") }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 12
                        ColumnLayout {
                            Layout.fillWidth: true; spacing: 4
                            Text { text: "Bootstrap DNS"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            DnsField {
                                text: App.dnsBootstrapServer
                                placeholderText: "1.1.1.1"
                                onEditingFinished: App.setBootstrapDns(text, "")
                            }
                        }
                        ColumnLayout {
                            spacing: 4
                            Text { text: I18n.t("Тип"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            StyledCombo {
                                width: 130
                                model: page.dnsTypes
                                currentIndex: page.idxIn(page.dnsTypes, App.dnsBootstrapType)
                                onActivated: App.setBootstrapDns("", currentText)
                            }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 12
                        ColumnLayout {
                            Layout.fillWidth: true; spacing: 4
                            Text { text: "Proxy DNS"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            DnsField {
                                text: App.dnsProxyServer
                                placeholderText: "8.8.8.8"
                                onEditingFinished: App.setProxyDns(text, "")
                            }
                        }
                        ColumnLayout {
                            spacing: 4
                            Text { text: I18n.t("Тип"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            StyledCombo {
                                width: 130
                                model: page.dnsTypes
                                currentIndex: page.idxIn(page.dnsTypes, App.dnsProxyType)
                                onActivated: App.setProxyDns("", currentText)
                            }
                        }
                    }
                }
            }

            // ---- routing presets -------------------------------------
            Text { text: I18n.t("Быстрые пресеты приоритета"); color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            Flow {
                Layout.fillWidth: true
                spacing: 8
                AccentButton { kind: "ghost"; glyph: "\uE774"; text: I18n.t("Всё через VPN"); onClicked: App.applyRoutingPreset("global") }
                AccentButton { kind: "ghost"; glyph: "\uE71B"; text: I18n.t("Только заблокированное"); onClicked: App.applyRoutingPreset("blocked") }
                AccentButton { kind: "ghost"; glyph: "\uE80F"; text: I18n.t("Всё кроме РФ"); onClicked: App.applyRoutingPreset("except_ru") }
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
                        text: I18n.t("Быстрые переключатели для популярных сервисов (YouTube, Discord и др.). «По пресету» снимает ручное правило и возвращает сервис под текущий пресет.")
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

        contentItem: Text {
            text: I18n.t("«Всё через VPN» отправляет весь трафик через сервер.\n«По моим правилам» использует ваши правила по приложениям, сервисам, доменам и IP.\n«Без VPN по умолчанию» пускает трафик напрямую, кроме явно указанных исключений.\n\nСервисы:\n«По пресету» — убрать ручное правило и снова доверить сервис текущему пресету.\n«Прокси» — всегда вести сервис через VPN/прокси, даже если общий режим прямой.\n«Прямой» — всегда вести сервис напрямую, даже если общий режим «Всё через VPN».\n\nПравила по приложениям работают только в режиме TUN.")
            color: Theme.textMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontNormal
            wrapMode: Text.WordWrap
            lineHeight: 1.25
            leftPadding: 20; rightPadding: 20; bottomPadding: 6
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
                text: I18n.t("Домен (example.com), маска (*.example.com), CIDR (10.0.0.0/8) или geosite/geoip-метка.")
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
                    placeholderText: "example.com|proxy\nads.example.net|block\ngeosite:category-ru|direct"
                    background: Rectangle { radius: Theme.radiusSmall; color: Theme.card; border.width: 1; border.color: Theme.borderSolid }
                }
            }
        }
    }
}
