import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

FluentScroll {
    id: page
    readonly property string usdtAddress: "TWHsuUDru4pXBcGpfeKfzymbkfajyFnb2s"
    roundedClip: false
    ColumnLayout {
        width: page.width
        spacing: 16

        // ── Title ──
        Text {
            text: App.appName
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontTitle
            font.weight: Font.DemiBold
        }

        // ── Version caption ──
        Text {
            text: "v" + App.appVersion
            color: Theme.textFaint
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }

        // ── Separator ──
        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.divider }

        // ── Descriptions ──
        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: 15
            text: I18n.t("Lumen — самостоятельный Windows-клиент для VPN/TUN, системного прокси, маршрутизации, управления серверами и обхода DPI (DPI bypass) через zapret.")
        }
        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.textMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontNormal
            text: I18n.t("Проект предлагает графический интерфейс на базе QML с эффектами Mica/Acrylic и аппаратным ускорением рендеринга.")
        }

        // ── Separator ──
        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.divider }

        // ── Links ──
        Text {
            text: I18n.t("Ссылки")
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontStrong
            font.weight: Font.DemiBold
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            AccentButton {
                kind: "accent"
                glyph: "\uE724"  // Send
                text: I18n.t("Telegram канал")
                onClicked: App.openUrl("https://telegram.me/lumenkvn")
            }
            AccentButton {
                kind: "ghost"
                glyph: "\uE734"  // Shield
                text: I18n.t("Рекомендуемый VPN")
                onClicked: App.openUrl("https://t.me/zapretvpns_bot")
            }
            AccentButton {
                kind: "ghost"
                glyph: "\uE8F1"
                text: I18n.t("Поддержать проект")
                onClicked: supportDialog.open()
            }
            Item { Layout.fillWidth: true }
        }

        Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
    }

    FluentDialog {
        id: supportDialog
        title: I18n.t("Поддержать проект")
        width: 430
        showOkButton: false
        cancelText: I18n.t("Закрыть")

        contentItem: ColumnLayout {
            spacing: 12
            Text {
                Layout.fillWidth: true
                text: I18n.t("Выберите удобный способ поддержки:")
                color: Theme.textMuted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontNormal
                wrapMode: Text.WordWrap
            }
            AccentButton {
                Layout.fillWidth: true
                kind: "accent"
                glyph: "\uE8F1"
                text: "DonationAlerts"
                onClicked: {
                    supportDialog.close()
                    App.openUrl("https://www.donationalerts.com/r/studiobebraedition")
                }
            }
            AccentButton {
                Layout.fillWidth: true
                kind: "ghost"
                glyph: "\uE8C7"
                text: "USDT (TRC20)"
                onClicked: {
                    supportDialog.close()
                    usdtDialog.open()
                }
            }
        }
    }

    FluentDialog {
        id: usdtDialog
        title: "USDT (TRC20)"
        width: 420
        showOkButton: false
        cancelText: I18n.t("Закрыть")

        contentItem: ColumnLayout {
            spacing: 12
            Image {
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: 250
                Layout.preferredHeight: 250
                source: "assets/usdt-trc20-qr.png"
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
            }
            FluentTextField {
                Layout.fillWidth: true
                text: page.usdtAddress
                readOnly: true
                selectByMouse: true
                horizontalAlignment: TextInput.AlignHCenter
            }
            AccentButton {
                Layout.alignment: Qt.AlignHCenter
                kind: "accent"
                glyph: "\uE8C8"
                text: I18n.t("Копировать адрес")
                onClicked: App.copyText(page.usdtAddress)
            }
        }
    }
}
