import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import QtQuick.Effects
import QtQuick.Dialogs
import App 1.0
import "."
Item {
    id: page
    readonly property var themeKeys: ["system", "light", "dark"]
    readonly property var languageKeys: App.availableLanguages
    readonly property var presetKeys: ["default", "absolutely", "ayu", "catppuccin", "codex", "dracula", "everforest", "github", "gruvbox", "linear", "lobster", "material", "matrix", "midnight", "monokai", "night-owl", "nord", "notion", "one", "oscurange", "raycast", "rose-pine", "sentry", "solarized", "temple", "tokyo-night", "vercel", "vscode-plus", "xcode"]
    readonly property var presetLabels: ["Default", "Absolutely", "Ayu", "Catppuccin", "Codex", "Dracula", "Everforest", "GitHub", "Gruvbox", "Linear", "Lobster", "Material", "Matrix", "Midnight", "Monokai", "Night Owl", "Nord", "Notion", "One", "Oscurange", "Raycast", "Rose Pine", "Sentry", "Solarized", "Temple", "Tokyo Night", "Vercel", "VS Code Plus", "Xcode"]
    property int currentTab: 0
    readonly property var tabModel: [
        { glyph: "\uE790", label: I18n.t("Внешний вид"),        pageIndex: 0, compactHidden: false },
        { glyph: "\uE774", label: I18n.t("Сеть"),               pageIndex: 1, compactHidden: false },
        { glyph: "\uE8AB", label: I18n.t("Авто-переключение"),  pageIndex: 2, compactHidden: true  },
        { glyph: "\uE7E8", label: I18n.t("Запуск и тесты"),     pageIndex: 3, compactHidden: false },
        { glyph: "\uE895", label: I18n.t("Обновления"),         pageIndex: 4, compactHidden: false },
        { glyph: "\uE74E", label: I18n.t("Данные"),             pageIndex: 5, compactHidden: true  }
    ]
    readonly property bool narrowTabs: width < 760
    readonly property bool showTabLabels: width >= 720
    readonly property real tabScale: width < 760 ? 0.82 : 1.0
    readonly property var visibleTabs: tabModel.filter(function(t) { return !(t.compactHidden && App.compactMode); })
    readonly property int activeVisibleIndex: {
        for (var i = 0; i < visibleTabs.length; i++)
            if (visibleTabs[i].pageIndex === currentTab) return i;
        return 0;
    }
    onVisibleTabsChanged: {
        var ok = false;
        for (var i = 0; i < visibleTabs.length; i++)
            if (visibleTabs[i].pageIndex === currentTab) { ok = true; break; }
        if (!ok && visibleTabs.length > 0) currentTab = visibleTabs[0].pageIndex;
    }



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
            Text { text: sr.title; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal; font.hintingPreference: Font.PreferVerticalHinting; renderType: Text.QtRendering; Layout.fillWidth: true; wrapMode: Text.WordWrap }
            Text { text: sr.subtitle; visible: sr.subtitle.length > 0; color: Theme.textFaint; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; font.hintingPreference: Font.PreferVerticalHinting; renderType: Text.QtRendering; Layout.fillWidth: true; wrapMode: Text.WordWrap }
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
            showSystem: true
            showDarkness: true
            onAccepted: (hex) => App.setAccent(hex)
        }
    }

    component BaseTintPicker: Item {
        id: bt
        implicitWidth: 56
        implicitHeight: Theme.controlHeight
        Rectangle {
            id: btSwatch
            anchors.fill: parent
            radius: Theme.radiusSmall
            color: App.uiBaseTint !== "" ? App.uiBaseTint : "transparent"
            border.width: 1
            border.color: btHover.hovered ? Theme.text : Theme.divider
            Text {
                anchors.centerIn: parent
                visible: App.uiBaseTint === ""
                text: "\u2014"
                font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal
                color: Theme.textFaint
            }
            HoverHandler { id: btHover }
            TapHandler {
                onTapped: {
                    var src = App.uiBaseTintSrc
                    if (src && src.indexOf("|") > 0) {
                        var parts = src.split("|")
                        baseTintDlg.openWith(parts[0], parseFloat(parts[1]))
                    } else if (App.uiBaseTint !== "") {
                        baseTintDlg.openWith(App.uiBaseTint, 0)
                    } else {
                        baseTintDlg.openWith("#5B6B8C", 0.6)
                    }
                }
            }
        }
        ColorPickerDialog {
            id: baseTintDlg
            showDarkness: true
            heading: I18n.t("Свой базовый тон")
            onAccepted: (hex) => {
                App.setUiBaseTint(hex)
                App.setUiBaseTintSrc(baseTintDlg._baseHex() + "|" + baseTintDlg._mute)
            }
        }
    }


    FileDialog {
        id: wallpaperDlg
        title: I18n.t("Выберите изображение")
        nameFilters: [I18n.t("Изображения") + " (*.png *.jpg *.jpeg *.bmp *.webp)"]
        onAccepted: App.setUiWallpaper(selectedFile.toString())
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.topMargin: 20
        anchors.bottomMargin: 8
        anchors.rightMargin: 12
        spacing: Theme.spacingLarge

        Text {
            text: I18n.t("Настройки")
            font.family: Theme.fontFamily; font.pixelSize: Theme.fontTitle
            font.weight: Font.DemiBold; color: Theme.text
        }

        // ===== Адаптивный таб-бар с анимированным индикатором =====
        Item {
            Layout.fillWidth: true
            implicitHeight: Math.round(40 * Theme.fontScale)

            RowLayout {
                id: tabRow
                anchors.fill: parent
                spacing: 4
                Repeater {
                    model: page.visibleTabs
                    delegate: Rectangle {
                        required property int index
                        required property var modelData
                        readonly property bool current: page.currentTab === modelData.pageIndex
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        Layout.fillHeight: true
                        radius: Theme.radiusSmall
                        color: "transparent"
                        Rectangle {
                            anchors.fill: parent
                            radius: parent.radius
                            color: current ? Theme.accentSoft : Theme.navHover
                            opacity: current || tabMouse.containsMouse ? 1 : 0
                            Behavior on opacity { NumberAnimation { duration: Theme.animations ? 140 : 0 } }
                        }
                        Row {
                            anchors.centerIn: parent
                            spacing: 8
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.glyph
                                font.family: "Segoe Fluent Icons"
                                font.pixelSize: Math.round(Theme.fontNormal * page.tabScale)
                                color: current ? Theme.accent : Theme.textMuted
                                Behavior on color { ColorAnimation { duration: Theme.animations ? 140 : 0 } }
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                visible: page.showTabLabels
                                text: modelData.label
                                font.family: Theme.fontFamily
                                font.pixelSize: Math.round(Theme.fontNormal * page.tabScale)
                                font.weight: current ? Font.DemiBold : Font.Normal
                                color: current ? Theme.text : Theme.textMuted
                                Behavior on color { ColorAnimation { duration: Theme.animations ? 140 : 0 } }
                            }
                        }
                        MouseArea {
                            id: tabMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: page.currentTab = modelData.pageIndex
                        }
                    }
                }
            }

            // Скользящий индикатор активной вкладки
            Rectangle {
                id: tabIndicator
                readonly property int count: page.visibleTabs.length
                readonly property real tw: (tabRow.width - tabRow.spacing * (count - 1)) / count
                height: 2.5
                radius: 2
                color: Theme.accent
                y: tabRow.height - height
                width: tw * 0.5
                x: page.activeVisibleIndex * (tw + tabRow.spacing) + (tw - width) / 2
                Behavior on x { NumberAnimation { duration: Theme.animations ? 240 : 0; easing.type: Theme.easeEmphasized } }
                Behavior on width { NumberAnimation { duration: Theme.animations ? 240 : 0 } }
            }
        }

        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.divider }

        SwipeView {
            id: tabStack
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            interactive: false
            currentIndex: page.currentTab

            Binding { target: tabStack.contentItem; property: "highlightMoveDuration"; value: Theme.animations ? 520 : 0 }
            Binding { target: tabStack.contentItem; property: "highlightResizeDuration"; value: Theme.animations ? 160 : 0 }
            Binding { target: tabStack.contentItem; property: "highlightMoveVelocity"; value: Theme.animations ? 2800 : -1 }

        FluentScroll {
            roundedClip: false
            ColumnLayout {
                width: parent.width
                spacing: Theme.spacingLarge
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
                    glyph: "\uE771"; title: I18n.t("Палитра (пресет)"); subtitle: I18n.t("Готовая цветовая тема поверх светлой/тёмной")
                    StyledCombo {
                        model: page.presetLabels
                        currentIndex: Math.max(0, page.presetKeys.indexOf(App.uiThemePreset))
                        onActivated: App.setUiThemePreset(page.presetKeys[currentIndex])
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
                    glyph: "\uE7E6"; title: I18n.t("Свой базовый тон"); subtitle: I18n.t("Кастомный цвет подложки окна (поверх пресета)")
                    AccentButton {
                        kind: "ghost"
                        text: I18n.t("Сбросить")
                        visible: App.uiBaseTint !== ""
                        onClicked: App.setUiBaseTint("")
                    }
                    BaseTintPicker {}
                }

                SettingRow {
                    glyph: "\uEB9F"; title: I18n.t("Обои окна"); subtitle: I18n.t("Фоновое изображение под интерфейсом")
                    AccentButton {
                        kind: "ghost"
                        text: I18n.t("Сбросить")
                        visible: App.uiWallpaper !== ""
                        onClicked: App.setUiWallpaper("")
                    }
                    AccentButton {
                        text: I18n.t("Выбрать\u2026")
                        onClicked: wallpaperDlg.open()
                    }
                }

                SettingRow {
                    visible: App.uiWallpaper !== ""
                    glyph: "\uE890"; title: I18n.t("Непрозрачность обоев"); subtitle: I18n.t("Насколько ярко видно изображение, %")
                    FluentSpin { from: 0; to: 100; stepSize: 5; value: App.uiWallpaperOpacity; onValueModified: App.setUiWallpaperOpacity(value) }
                }

                SettingRow {
                    visible: App.uiWallpaper !== ""
                    glyph: "\uE7B3"; title: I18n.t("Размытие обоев"); subtitle: I18n.t("Сила размытия фона, 0–100")
                    FluentSpin { from: 0; to: 100; stepSize: 5; value: App.uiWallpaperBlur; onValueModified: App.setUiWallpaperBlur(value) }
                }

                SettingRow {
                    visible: App.uiWallpaper !== ""
                    glyph: "\uE706"; title: I18n.t("Яркость обоев"); subtitle: I18n.t("100 = оригинал, меньше = темнее")
                    FluentSpin { from: 0; to: 100; stepSize: 5; value: App.uiWallpaperBrightness; onValueModified: App.setUiWallpaperBrightness(value) }
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
                    glyph: "\uE8E9"; title: I18n.t("Масштаб интерфейса"); subtitle: I18n.t("Размер шрифта и всего интерфейса, % (80–140)")
                    FluentSpin { from: 80; to: 140; stepSize: 5; value: App.uiFontScale; onValueModified: App.setUiFontScale(value) }
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
                    glyph: "\uE790"; title: I18n.t("Прозрачность"); subtitle: I18n.t("Использовать системные эффекты фона Windows 11")
                    Switch {
                        checked: App.uiBackdrop !== "solid"
                        onToggled: App.setUiBackdrop(checked ? "mica" : "solid")
                    }
                }

                SettingRow {
                    enabled: App.uiBackdrop !== "solid"
                    opacity: enabled ? 1.0 : 0.55
                    glyph: "\uE9E9"; title: I18n.t("Сила прозрачности"); subtitle: I18n.t("0: Mica почти выключена, 100: максимум Mica")
                    Slider {
                        Layout.preferredWidth: 170
                        from: 0
                        to: 100
                        stepSize: 5
                        value: App.uiTransparencyStrength
                        onMoved: App.setUiTransparencyStrength(Math.round(value))
                    }
                    Text {
                        text: App.uiTransparencyStrength + "%"
                        color: Theme.textMuted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        Layout.preferredWidth: 42
                        horizontalAlignment: Text.AlignRight
                    }
                }

                SettingRow {
                    glyph: "\uE890"; title: I18n.t("Компактный режим"); subtitle: I18n.t("Скрыть продвинутые настройки и второстепенные разделы")
                    Switch { checked: App.compactMode; onToggled: App.setInterfaceMode(checked ? "compact" : "full") }
                }
            }
        }
                Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
            }
        }

        FluentScroll {
            roundedClip: false
            ColumnLayout {
                width: parent.width
                spacing: Theme.spacingLarge
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
                    glyph: "\uE968"; title: I18n.t("Предпочитать IPv6"); subtitle: I18n.t("Использовать IPv6-адреса раньше IPv4, если сервер и сеть это поддерживают")
                    Switch { checked: App.preferIpv6; onToggled: App.setPreferIpv6(checked) }
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
                Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
            }
        }

        FluentScroll {
            roundedClip: false
            ColumnLayout {
                width: parent.width
                spacing: Theme.spacingLarge
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
                Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
            }
        }

        FluentScroll {
            roundedClip: false
            ColumnLayout {
                width: parent.width
                spacing: Theme.spacingLarge
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
                    enabled: App.launchOnStartup
                    opacity: enabled ? 1.0 : 0.55
                    glyph: "\uE7E8"; title: I18n.t("Запускать автозапуск в трей"); subtitle: I18n.t("При входе в Windows не показывать окно, только иконку в трее")
                    Switch { checked: App.launchInTrayOnStartup; onToggled: App.setLaunchInTrayOnStartup(checked) }
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
                        Popup {
                            id: speedPresetMenu
                            y: speedPresetBtn.height + 4
                            x: speedPresetBtn.width - width
                            width: 230
                            padding: 4
                            modal: false
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
                Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
            }
        }

        FluentScroll {
            roundedClip: false
            ColumnLayout {
                width: parent.width
                spacing: Theme.spacingLarge
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
                    glyph: "\uE895"; title: I18n.t("Автообновление приложения"); subtitle: I18n.t("Автоматически скачивать и устанавливать новые версии")
                    Switch { checked: App.appAutoUpdate; onToggled: App.setAppAutoUpdate(checked) }
                }
                SettingRow {
                    glyph: "\uEBD3"; title: I18n.t("Авто-обновление Xray"); subtitle: I18n.t("Обновлять ядро Xray автоматически")
                    Switch { checked: App.xrayAutoUpdate; onToggled: App.setXrayAutoUpdate(checked) }
                }
            }
        }
                Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
            }
        }

        FluentScroll {
            roundedClip: false
            ColumnLayout {
                width: parent.width
                spacing: Theme.spacingLarge
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
                SettingRow {
                    glyph: "\uE9D9"; title: I18n.t("Телеметрия"); subtitle: I18n.t("Отправлять диагностические ошибки на сервер Lumen KVN")
                    Switch { checked: App.diagnosticsUploadEnabled; onToggled: App.setDiagnosticsUpload(checked) }
                }
            }
        }
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
        Text {
            Layout.topMargin: 4
            text: App.appName + " · v" + App.appVersion
            color: Theme.textFaint
            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
        }
                Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
            }
        }
        }
    }
}
