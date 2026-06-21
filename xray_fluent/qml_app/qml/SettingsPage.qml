import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."
//grouped into Внешний вид / Сеть / Авто-переключение / Пути к ядрам /
// Запуск / Обновления / Данные. Все ряды подключены к
// AppBridge. Компактный режим скрывает продвинутые группы (авто-переключение,
// пути к ядрам, данные) — как SettingsPage.set_compact_mode в
// классическом приложении — вместо сворачивания навигационной панели.
FluentScroll {
    id: page
    readonly property var themeKeys: ["system", "light", "dark"]
    readonly property var languageKeys: App.availableLanguages

    component StyledCombo: FluentCombo { Layout.preferredWidth: 210 }

    component StyledField: TextField {
        implicitHeight: Theme.controlHeight
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontNormal
        opacity: enabled ? 1.0 : 0.5
        selectByMouse: true
        leftPadding: 10; rightPadding: 10
        background: Rectangle {
            radius: Theme.radiusSmall
            color: Theme.card
            border.width: 1; border.color: Theme.borderSolid
        }
    }

    component InfoIcon: ToolButton {
        property string tip: ""
        Layout.preferredWidth: 28
        Layout.preferredHeight: 28
        text: "\uE946"
        font.family: "Segoe Fluent Icons"
        font.pixelSize: 14
        hoverEnabled: true
        ToolTip.visible: hovered
        ToolTip.delay: 250
        ToolTip.text: tip
        contentItem: Text {
            text: parent.text
            font: parent.font
            color: parent.hovered ? Theme.text : Theme.textMuted
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: Theme.radiusSmall
            color: parent.hovered ? Theme.controlFillHover : "transparent"
            border.width: 0
        }
    }

    component SettingRow: RowLayout {
        id: sr
        property string glyph: ""
        property string title: ""
        property string subtitle: ""
        default property alias rightItems: rc.data
        Layout.fillWidth: true
        spacing: 12
        Text {
            text: sr.glyph
            visible: sr.glyph.length > 0
            font.family: "Segoe Fluent Icons"; font.pixelSize: 16
            color: Theme.textMuted
            Layout.preferredWidth: 22
            Layout.alignment: Qt.AlignVCenter
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            Text { text: sr.title; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; Layout.fillWidth: true; wrapMode: Text.WordWrap }
            Text { text: sr.subtitle; visible: sr.subtitle.length > 0; color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.fillWidth: true; wrapMode: Text.WordWrap }
        }
        RowLayout { id: rc; spacing: 8; Layout.alignment: Qt.AlignVCenter }
    }

    // ---- accent colour picker ----
    component AccentPicker: Item {
        id: ap
        implicitWidth: 56
        implicitHeight: Theme.controlHeight
        Rectangle {
            id: swatch
            anchors.fill: parent
            radius: Theme.radiusSmall
            color: App.accentColor
            border.width: 1
            border.color: hover.hovered ? Theme.text : Theme.divider
            HoverHandler { id: hover }
            TapHandler { onTapped: accentDlg.openWith(App.accentColor) }
        }
        ColorPickerDialog {
            id: accentDlg
            onAccepted: (hex) => App.setAccent(hex)
        }
    }

    ColumnLayout {
        width: page.width
        spacing: Theme.spacingLarge

        Text {
            text: I18n.t("Настройки")
            font.family: Theme.fontFamily; font.pixelSize: Theme.fontTitle
            font.weight: Font.DemiBold; color: Theme.text
        }

        // ============================ Внешний вид ============================
        Card {
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: I18n.t("Внешний вид"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE790"; title: I18n.t("Тема"); subtitle: I18n.t("Выберите светлую, тёмную или системную тему")
                    StyledCombo {
                        model: [I18n.t("Авто"), I18n.t("Светлая"), I18n.t("Тёмная")]
                        currentIndex: Math.max(0, page.themeKeys.indexOf(App.themeName))
                        onActivated: App.setTheme(page.themeKeys[currentIndex])
                    }
                }

                SettingRow {
                    glyph: "\uE774"; title: I18n.t("Язык"); subtitle: I18n.t("При первом запуске выбирается по языку системы")
                    StyledCombo {
                        model: App.languageLabels
                        currentIndex: Math.max(0, page.languageKeys.indexOf(App.language))
                        onActivated: App.setLanguage(page.languageKeys[currentIndex])
                    }
                }

                SettingRow {
                    glyph: "\uE2B1"; title: I18n.t("Цвет акцента"); subtitle: I18n.t("Цвет акцента для элементов интерфейса")
                    AccentPicker {}
                }

                SettingRow {
                    glyph: "\uE799"; title: I18n.t("Плотность"); subtitle: I18n.t("Насколько компактно расположены элементы")
                    StyledCombo {
                        model: [I18n.t("Компактная"), I18n.t("Обычная"), I18n.t("Просторная")]
                        readonly property var keys: ["compact", "comfortable", "spacious"]
                        currentIndex: Math.max(0, keys.indexOf(App.uiDensity))
                        onActivated: App.setUiDensity(keys[currentIndex])
                    }
                }

                SettingRow {
                    glyph: "\uE8B3"; title: I18n.t("Скругление углов"); subtitle: I18n.t("Радиус скругления карточек и кнопок, px")
                    FluentSpin { from: 0; to: 20; value: App.uiCornerRadius; onValueModified: App.setUiCornerRadius(value) }
                }

                SettingRow {
                    glyph: "\uE945"; title: I18n.t("Анимации"); subtitle: I18n.t("Плавные переходы и движение интерфейса")
                    Switch { checked: App.uiAnimations; onToggled: App.setUiAnimations(checked) }
                }

                SettingRow {
                    glyph: "\uE890"; title: I18n.t("Компактный режим"); subtitle: I18n.t("Скрыть продвинутые настройки и второстепенные разделы")
                    Switch { checked: App.compactMode; onToggled: App.setInterfaceMode(checked ? "compact" : "full") }
                }
            }
        }

        // ============================ Сеть ============================
        Card {
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: I18n.t("Сеть"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE80F"; title: I18n.t("Прокси в обход локальной сети"); subtitle: I18n.t("Не проксировать адреса локальной сети")
                    Switch { checked: App.proxyBypassLan; onToggled: App.setProxyBypassLan(checked) }
                }
                SettingRow {
                    glyph: "\uE895"; title: I18n.t("Переподключение при смене сети"); subtitle: I18n.t("Автоматически переподключаться при изменении сети")
                    Switch { checked: App.reconnectOnNetworkChange; onToggled: App.setReconnectOnNetworkChange(checked) }
                }
                SettingRow {
                    glyph: "\uE72E"; title: I18n.t("Kill-switch"); subtitle: I18n.t("При обрыве VPN блокировать трафик вместо выхода напрямую")
                    Switch { checked: App.killSwitch; onToggled: App.setKillSwitch(checked) }
                }
                SettingRow {
                    glyph: "\uE72C"; title: I18n.t("Проверять обновления ядра и geoip/geosite"); subtitle: I18n.t("При запуске проверять обновления и уведомлять")
                    Switch { checked: App.resourceUpdateCheck; onToggled: App.setResourceUpdateCheck(checked) }
                }
                SettingRow {
                    visible: !App.compactMode
                    Layout.fillWidth: true
                    glyph: "\uE8A6"; title: I18n.t("Фрагментирование"); subtitle: I18n.t("TLS Fragment")
                    InfoIcon { tip: I18n.t("Фрагментирование делит TLS-рукопожатие на части. Может помочь обходу DPI, но иногда снижает стабильность.") }
                    Switch { checked: App.fragmentationEnabled; onToggled: App.setFragmentation(checked) }
                }
                SettingRow {
                    visible: !App.compactMode && App.fragmentationEnabled
                    Layout.fillWidth: true
                    glyph: "\uE9D9"; title: I18n.t("Настройки фрагментирования"); subtitle: I18n.t("Fragment Settings")
                    InfoIcon { tip: I18n.t("Packets выбирает часть трафика, Length размер фрагментов, Delay задержку между ними.") }
                    StyledField { id: fragPackets; Layout.preferredWidth: 100; text: App.fragmentPackets; placeholderText: "tlshello"; onEditingFinished: App.setFragmentSettings(text, fragLength.text, fragDelay.text) }
                    StyledField { id: fragLength; Layout.preferredWidth: 100; text: App.fragmentLength; placeholderText: "50-100"; onEditingFinished: App.setFragmentSettings(fragPackets.text, text, fragDelay.text) }
                    StyledField { id: fragDelay; Layout.preferredWidth: 100; text: App.fragmentDelay; placeholderText: "10-20"; onEditingFinished: App.setFragmentSettings(fragPackets.text, fragLength.text, text) }
                }
                SettingRow {
                    visible: !App.compactMode
                    Layout.fillWidth: true
                    glyph: "\uE7C3"; title: I18n.t("Мультиплексирование"); subtitle: I18n.t("Multiplex")
                    InfoIcon { tip: I18n.t("Мультиплексирование объединяет несколько запросов в один канал. Может ускорить много мелких соединений, но зависит от поддержки сервера.") }
                    Switch { checked: App.multiplexingEnabled; onToggled: App.setMultiplexing(checked) }
                }
                SettingRow {
                    visible: !App.compactMode && App.multiplexingEnabled
                    Layout.fillWidth: true
                    glyph: "\uE9D9"; title: I18n.t("Настройки мультиплексирования"); subtitle: I18n.t("Multiplex Settings")
                    InfoIcon { tip: I18n.t("Потоки задают, сколько запросов можно вести внутри одного мультиплексированного соединения.") }
                    FluentSpin { from: 1; to: 32; value: App.multiplexConcurrency; onValueModified: App.setMultiplexConcurrency(value) }
                }
            }
        }

        // ============================ Авто-переключение ============================
        Card {
            Layout.fillWidth: true
            visible: !App.compactMode
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: I18n.t("Авто-переключение"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE895"; title: I18n.t("Включить авто-переключение"); subtitle: I18n.t("Переключать сервер при падении скорости")
                    Switch { checked: App.autoSwitchEnabled; onToggled: App.setAutoSwitch(checked) }
                }

                SettingRow {
                    glyph: "\uEC4A"; title: I18n.t("Порог скорости"); subtitle: I18n.t("КБ/с, ниже которого срабатывает переключение")
                    FluentSpin { from: 1; to: 10000; value: App.autoSwitchThreshold; onValueModified: App.setAutoSwitchThreshold(value) }
                }
                SettingRow {
                    glyph: "\uE916"; title: I18n.t("Задержка"); subtitle: I18n.t("Секунд низкой скорости перед переключением")
                    FluentSpin { from: 5; to: 300; value: App.autoSwitchDelay; onValueModified: App.setAutoSwitchDelay(value) }
                }
                SettingRow {
                    glyph: "\uE81C"; title: I18n.t("Пауза"); subtitle: I18n.t("Секунд между переключениями")
                    FluentSpin { from: 10; to: 600; value: App.autoSwitchCooldown; onValueModified: App.setAutoSwitchCooldown(value) }
                }
            }
        }

        // ============================ Пути к ядрам ============================
        Card {
            Layout.fillWidth: true
            visible: !App.compactMode
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: I18n.t("Пути к ядрам"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE756"; title: I18n.t("Ядро Xray"); subtitle: I18n.t("Путь к исполняемому файлу xray")
                    StyledField { id: xrayField; Layout.preferredWidth: 240; text: App.xrayPath; placeholderText: "core\\xray.exe"; onEditingFinished: App.setXrayPath(text) }
                    AccentButton { kind: "ghost"; glyph: "\uE8B7"; text: I18n.t("Обзор"); onClicked: { var p = App.browseXrayPath(); if (p) xrayField.text = p } }
                }
                SettingRow {
                    glyph: "\uE756"; title: I18n.t("Ядро sing-box-extended"); subtitle: I18n.t("Путь к исполняемому файлу sing-box-extended")
                    StyledField { id: sbField; Layout.preferredWidth: 240; text: App.singboxPath; placeholderText: "core\\sing-box.exe"; onEditingFinished: App.setSingboxPath(text) }
                    AccentButton { kind: "ghost"; glyph: "\uE8B7"; text: I18n.t("Обзор"); onClicked: { var p = App.browseSingboxPath(); if (p) sbField.text = p } }
                }
            }
        }

        // ============================ Запуск ============================
        Card {
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: I18n.t("Запуск"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE7E8"; title: I18n.t("Запускать при входе"); subtitle: I18n.t("Автозапуск вместе с Windows")
                    Switch { checked: App.launchOnStartup; onToggled: App.setLaunchOnStartup(checked) }
                }
                SettingRow {
                    glyph: "\uE768"; title: I18n.t("Автоподключение после запуска"); subtitle: I18n.t("Подключаться к последнему использованному серверу при старте")
                    Switch { checked: App.autoConnectLast; onToggled: App.setAutoConnectLast(checked) }
                }
                SettingRow {
                    glyph: "\uE896"; title: I18n.t("Автоподключение при импорте"); subtitle: I18n.t("Сразу подключаться к импортированному серверу")
                    Switch { checked: App.autoConnectOnImport; onToggled: App.setAutoConnectOnImport(checked) }
                }
                SettingRow {
                    glyph: "\uE945"; title: I18n.t("Автозапуск Zapret"); subtitle: I18n.t("Запускать Zapret при старте приложения")
                    Switch { checked: App.zapretAutostart; onToggled: App.setZapretAutostart(checked) }
                }
            }
        }

        // ============================ Тестирование серверов ============================
        Card {
            Layout.fillWidth: true
            visible: !App.compactMode
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: I18n.t("Тестирование серверов"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE724"; title: I18n.t("Способ измерения ping"); subtitle: I18n.t("TCP ping быстрее всего; ICMP — системный ping; реальная задержка — HTTP через сервер")
                    StyledCombo {
                        model: [I18n.t("TCP ping (быстро)"), I18n.t("ICMP (системный)"), I18n.t("Реальная задержка (HTTP)")]
                        readonly property var keys: ["tcping", "icmp", "real"]
                        currentIndex: Math.max(0, keys.indexOf(App.pingMethod))
                        onActivated: App.setPingMethod(keys[currentIndex])
                    }
                }
                SettingRow {
                    glyph: "\uEC4A"; title: I18n.t("URL теста скорости"); subtitle: I18n.t("Пусто — стандартный (cachefly 50MB)")
                    StyledField {
                        id: speedUrlField
                        Layout.preferredWidth: 300
                        text: App.speedTestUrl
                        placeholderText: "https://cachefly.cachefly.net/50mb.test"
                        onEditingFinished: App.setSpeedTestUrl(text)
                    }
                    Button {
                        id: speedPresetBtn
                        implicitHeight: Theme.controlHeight
                        implicitWidth: 36
                        hoverEnabled: true
                        text: "\uE70D"
                        font.family: "Segoe Fluent Icons"; font.pixelSize: 12
                        background: Rectangle {
                            radius: Theme.radiusSmall
                            color: speedPresetBtn.down ? Theme.controlFillPressed
                                   : (speedPresetBtn.hovered ? Theme.controlFillHover : Theme.controlFill)
                            border.width: 1; border.color: Theme.borderSolid
                        }
                        contentItem: Text {
                            text: speedPresetBtn.text; font: speedPresetBtn.font
                            color: Theme.textMuted
                            horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
                        }
                        onClicked: speedPresetMenu.visible ? speedPresetMenu.close() : speedPresetMenu.open()
                        function pick(u) { App.setSpeedTestUrl(u); speedUrlField.text = u; }
                        // Fluent-flyout в стиле FluentCombo: скруглённый, Theme.flyout, строки-«пилюли» с hover.
                        Popup {
                            id: speedPresetMenu
                            y: speedPresetBtn.height + 4
                            x: speedPresetBtn.width - width
                            width: 230
                            padding: 4
                            modal: false
                            // Клик по самой кнопке не должен закрывать-и-сразу-открывать список:
                            // считаем «вне поповера» только клики за пределами кнопки-родителя,
                            // тогда onClicked корректно переключает open/close (как у FluentCombo).
                            parent: speedPresetBtn
                            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                            readonly property var presets: [
                                { label: "Cachefly 100 MB",  url: "https://cachefly.cachefly.net/100mb.test" },
                                { label: "Cachefly 50 MB",   url: "https://cachefly.cachefly.net/50mb.test" },
                                { label: "Cachefly 10 MB",   url: "https://cachefly.cachefly.net/10mb.test" },
                                { label: "Cachefly 1 MB",    url: "https://cachefly.cachefly.net/1mb.test" },
                                { label: "Cloudflare 100 MB", url: "https://speed.cloudflare.com/__down?bytes=104857600" },
                                { label: "Cloudflare 50 MB",  url: "https://speed.cloudflare.com/__down?bytes=52428800" },
                                { label: "Cloudflare 10 MB",  url: "https://speed.cloudflare.com/__down?bytes=10485760" }
                            ]
                            background: Rectangle {
                                radius: 8
                                color: Theme.flyout
                                border.width: 1
                                border.color: Theme.flyoutBorder
                            }
                            contentItem: Column {
                                spacing: 0
                                Repeater {
                                    model: speedPresetMenu.presets
                                    delegate: ItemDelegate {
                                        id: presetItem
                                        width: speedPresetMenu.availableWidth
                                        height: 34
                                        padding: 0
                                        hoverEnabled: true
                                        contentItem: Text {
                                            leftPadding: 12
                                            rightPadding: 12
                                            text: modelData.label
                                            color: Theme.text
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontNormal
                                            verticalAlignment: Text.AlignVCenter
                                            elide: Text.ElideRight
                                        }
                                        background: Rectangle {
                                            anchors.fill: parent
                                            anchors.margins: 3
                                            radius: Theme.radiusSmall
                                            color: presetItem.hovered ? Theme.cardHover : "transparent"
                                            Rectangle {
                                                visible: App.speedTestUrl === modelData.url
                                                width: 3
                                                radius: 1.5
                                                height: parent.height * 0.5
                                                anchors.left: parent.left
                                                anchors.leftMargin: 1
                                                anchors.verticalCenter: parent.verticalCenter
                                                color: Theme.accent
                                            }
                                        }
                                        onClicked: { speedPresetBtn.pick(modelData.url); speedPresetMenu.close() }
                                    }
                                }
                            }
                            enter: Transition {
                                NumberAnimation { property: "opacity"; from: 0.0; to: 1.0; duration: 90 }
                            }
                        }
                    }
                }
                SettingRow {
                    glyph: "\uE8C9"; title: I18n.t("Параллельных проверок"); subtitle: I18n.t("Сколько серверов тестировать одновременно (0 — авто)")
                    FluentSpin { from: 0; to: 32; value: App.speedTestConcurrency; onValueModified: App.setSpeedTestConcurrency(value) }
                }
            }
        }

        // ============================ Обновления ============================
        Card {
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: I18n.t("Обновления"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE895"; title: I18n.t("Автообновление подписок"); subtitle: I18n.t("Как часто фоново обновлять подписки (прокси/VPN не отключается)")
                    StyledCombo {
                        Layout.preferredWidth: 200
                        readonly property var values: [0, 30, 60, 240, 720, 1440]
                        model: [I18n.t("Выкл"), I18n.t("30 мин"), I18n.t("1 час"), I18n.t("4 часа"), I18n.t("12 часов"), I18n.t("24 часа")]
                        currentIndex: Math.max(0, values.indexOf(App.subscriptionAutoUpdateMinutes))
                        onActivated: App.setSubscriptionAutoUpdateMinutes(values[currentIndex])
                    }
                }
                SettingRow {
                    glyph: "\uE777"; title: I18n.t("Проверять обновления"); subtitle: I18n.t("Автоматически проверять наличие обновлений")
                    Switch { checked: App.checkUpdates; onToggled: App.setCheckUpdates(checked) }
                }
                SettingRow {
                    glyph: "\uE896"; title: I18n.t("Разрешить установку"); subtitle: I18n.t("Скачивать и устанавливать обновления")
                    Switch { checked: App.allowUpdates; onToggled: App.setAllowUpdates(checked) }
                }
                SettingRow {
                    glyph: "\uE895"; title: I18n.t("Автообновление приложения"); subtitle: I18n.t("Автоматически скачивать и устанавливать новые версии")
                    Switch { checked: App.appAutoUpdate; onToggled: App.setAppAutoUpdate(checked) }
                }
                SettingRow {
                    glyph: "\uEBD3"; title: I18n.t("Авто-обновление Xray"); subtitle: I18n.t("Обновлять ядро Xray автоматически")
                    Switch { checked: App.xrayAutoUpdate; onToggled: App.setXrayAutoUpdate(checked) }
                }
            }
        }

        // ============================ Данные ============================
        Card {
            Layout.fillWidth: true
            visible: !App.compactMode
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: I18n.t("Данные"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE74E"; title: I18n.t("Резервная копия"); subtitle: I18n.t("Экспорт и импорт настроек и серверов")
                    AccentButton { kind: "ghost"; glyph: "\uE74E"; text: I18n.t("Экспорт"); onClicked: App.exportBackup() }
                    AccentButton { kind: "ghost"; glyph: "\uE8B7"; text: I18n.t("Импорт"); onClicked: App.importBackup() }
                }
            }
        }

        Text {
            Layout.topMargin: 4
            text: App.appName + " · v" + App.appVersion
            color: Theme.textFaint
            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
        }
        Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
    }
}
