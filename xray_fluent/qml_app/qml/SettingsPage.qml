import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

// Faithful port of ui/settings_page.py: SettingCardGroup + SettingCard rows
// grouped into Внешний вид / Сеть / Авто-переключение / Пути к ядрам /
// Запуск / Обновления / Данные / Безопасность. Все ряды подключены к
// AppBridge. Компактный режим скрывает продвинутые группы (авто-переключение,
// пути к ядрам, данные, безопасность) — как SettingsPage.set_compact_mode в
// классическом приложении — вместо сворачивания навигационной панели.
FluentScroll {
    id: page
    clip: true

    readonly property var themeKeys: ["system", "light", "dark"]

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
    // Like qfluentwidgets' ColorPickerButton: a plain colour chip (no hex code)
    // that opens the in-window Fluent colour dialog overlay (ColorPickerDialog).
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
            text: "Настройки"
            font.family: Theme.fontFamily; font.pixelSize: Theme.fontTitle
            font.weight: Font.DemiBold; color: Theme.text
        }

        // ============================ Внешний вид ============================
        Card {
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: "Внешний вид"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE790"; title: "Тема"; subtitle: "Выберите светлую, тёмную или системную тему"
                    StyledCombo {
                        model: ["Авто", "Светлая", "Тёмная"]
                        currentIndex: Math.max(0, page.themeKeys.indexOf(App.themeName))
                        onActivated: App.setTheme(page.themeKeys[currentIndex])
                    }
                }

                SettingRow {
                    glyph: "\uE2B1"; title: "Цвет акцента"; subtitle: "Цвет акцента для элементов интерфейса"
                    AccentPicker {}
                }

                SettingRow {
                    glyph: "\uE890"; title: "Компактный режим"; subtitle: "Скрыть продвинутые настройки и второстепенные разделы"
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
                Text { text: "Сеть"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE80F"; title: "Прокси в обход локальной сети"; subtitle: "Не проксировать адреса локальной сети"
                    Switch { checked: App.proxyBypassLan; onToggled: App.setProxyBypassLan(checked) }
                }
                SettingRow {
                    glyph: "\uE895"; title: "Переподключение при смене сети"; subtitle: "Автоматически переподключаться при изменении сети"
                    Switch { checked: App.reconnectOnNetworkChange; onToggled: App.setReconnectOnNetworkChange(checked) }
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
                Text { text: "Авто-переключение"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE895"; title: "Включить авто-переключение"; subtitle: "Переключать сервер при падении скорости"
                    Switch { checked: App.autoSwitchEnabled; onToggled: App.setAutoSwitch(checked) }
                }

                SettingRow {
                    glyph: "\uEC4A"; title: "Порог скорости"; subtitle: "КБ/с, ниже которого срабатывает переключение"
                    enabled: App.autoSwitchEnabled
                    SpinBox { from: 1; to: 10000; value: App.autoSwitchThreshold; onValueModified: App.setAutoSwitchThreshold(value) }
                }
                SettingRow {
                    glyph: "\uE916"; title: "Задержка"; subtitle: "Секунд низкой скорости перед переключением"
                    enabled: App.autoSwitchEnabled
                    SpinBox { from: 5; to: 300; value: App.autoSwitchDelay; onValueModified: App.setAutoSwitchDelay(value) }
                }
                SettingRow {
                    glyph: "\uE81C"; title: "Пауза"; subtitle: "Секунд между переключениями"
                    enabled: App.autoSwitchEnabled
                    SpinBox { from: 10; to: 600; value: App.autoSwitchCooldown; onValueModified: App.setAutoSwitchCooldown(value) }
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
                Text { text: "Пути к ядрам"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE756"; title: "Ядро Xray"; subtitle: "Путь к исполняемому файлу xray"
                    StyledField { id: xrayField; Layout.preferredWidth: 240; text: App.xrayPath; placeholderText: "встроенный"; onEditingFinished: App.setXrayPath(text) }
                    AccentButton { kind: "ghost"; glyph: "\uE8B7"; text: "Обзор"; onClicked: { var p = App.browseXrayPath(); if (p) xrayField.text = p } }
                }
                SettingRow {
                    glyph: "\uE756"; title: "Ядро sing-box"; subtitle: "Путь к исполняемому файлу sing-box"
                    StyledField { id: sbField; Layout.preferredWidth: 240; text: App.singboxPath; placeholderText: "встроенный"; onEditingFinished: App.setSingboxPath(text) }
                    AccentButton { kind: "ghost"; glyph: "\uE8B7"; text: "Обзор"; onClicked: { var p = App.browseSingboxPath(); if (p) sbField.text = p } }
                }
                SettingRow {
                    glyph: "\uEC7A"; title: "Движок TUN"; subtitle: "Ядро для режима TUN"
                    StyledCombo {
                        model: ["sing-box (default)", "tun2socks"]
                        readonly property var keys: ["singbox", "tun2socks"]
                        currentIndex: Math.max(0, keys.indexOf(App.tunEngine))
                        onActivated: App.setTunEngine(keys[currentIndex])
                    }
                }
            }
        }

        // ============================ Запуск ============================
        Card {
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: "Запуск"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE7E8"; title: "Запускать при входе"; subtitle: "Автозапуск вместе с Windows"
                    Switch { checked: App.launchOnStartup; onToggled: App.setLaunchOnStartup(checked) }
                }
                SettingRow {
                    glyph: "\uE7EF"; title: "Всегда запускать от администратора"; subtitle: "Windows будет запрашивать повышенные права при следующем запуске"
                    Switch { checked: App.alwaysRunAsAdmin; onToggled: App.setAlwaysRunAsAdmin(checked) }
                }
                SettingRow {
                    glyph: "\uE945"; title: "Автозапуск Zapret"; subtitle: "Запускать Zapret при старте приложения"
                    Switch { checked: App.zapretAutostart; onToggled: App.setZapretAutostart(checked) }
                }
            }
        }

        // ============================ Обновления ============================
        Card {
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: "Обновления"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE777"; title: "Проверять обновления"; subtitle: "Автоматически проверять наличие обновлений"
                    Switch { checked: App.checkUpdates; onToggled: App.setCheckUpdates(checked) }
                }
                SettingRow {
                    glyph: "\uE896"; title: "Разрешить установку"; subtitle: "Скачивать и устанавливать обновления"
                    Switch { checked: App.allowUpdates; onToggled: App.setAllowUpdates(checked) }
                }
                SettingRow {
                    glyph: "\uEBD3"; title: "Авто-обновление Xray"; subtitle: "Обновлять ядро Xray автоматически"
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
                Text { text: "Данные"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uE928"; title: "Шифрование данных"; subtitle: App.encryptionActive ? "Шифрование включено" : "Данные хранятся без шифрования"
                    StyledField { id: encField; Layout.preferredWidth: 180; echoMode: TextInput.Password; placeholderText: "Пароль" }
                    AccentButton { kind: "accent"; text: "Включить"; onClicked: { App.setEncryptionPassword(encField.text); encField.text = "" } }
                    AccentButton { kind: "danger"; text: "Отключить"; enabled: App.encryptionActive; onClicked: App.disableEncryption() }
                }
                SettingRow {
                    glyph: "\uE74E"; title: "Резервная копия"; subtitle: "Экспорт и импорт настроек и серверов"
                    AccentButton { kind: "ghost"; glyph: "\uE74E"; text: "Экспорт"; onClicked: App.exportBackup() }
                    AccentButton { kind: "ghost"; glyph: "\uE8B7"; text: "Импорт"; onClicked: App.importBackup() }
                }
            }
        }

        // ============================ Безопасность ============================
        Card {
            Layout.fillWidth: true
            visible: !App.compactMode
            ColumnLayout {
                anchors.fill: parent
                spacing: 14
                Text { text: "Безопасность"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold }

                SettingRow {
                    glyph: "\uEB95"; title: "Мастер-пароль"; subtitle: App.masterPasswordEnabled ? "Введите текущий пароль, чтобы отключить" : "Защитите приложение паролем"
                    StyledField { id: pwField; Layout.preferredWidth: 160; echoMode: TextInput.Password; placeholderText: "Пароль" }
                    AccentButton { kind: "accent"; text: "Установить"; onClicked: { App.setMasterPassword(pwField.text); pwField.text = "" } }
                    AccentButton { kind: "danger"; text: "Отключить"; enabled: App.masterPasswordEnabled; onClicked: { if (App.disableMasterPassword(pwField.text)) pwField.text = "" } }
                    AccentButton { kind: "ghost"; glyph: "\uE72E"; text: "Заблокировать"; enabled: App.masterPasswordEnabled; onClicked: App.lockNow() }
                }
                SettingRow {
                    glyph: "\uE916"; title: "Авто-блокировка"; subtitle: "Минут бездействия до блокировки"
                    enabled: App.masterPasswordEnabled
                    SpinBox { from: 1; to: 120; value: App.autoLockMinutes; onValueModified: App.setAutoLockMinutes(value) }
                }
            }
        }

        // ============================ Подвал ============================
        Text {
            Layout.topMargin: 4
            text: App.appName + " · v" + App.appVersion
            color: Theme.textFaint
            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
        }
        Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
    }
}
