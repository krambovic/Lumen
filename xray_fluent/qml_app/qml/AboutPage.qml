import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

FluentScroll {
    id: page
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
            text: I18n.t("Lumen KVN — самостоятельный Windows-клиент для VPN/TUN, системного прокси, маршрутизации, управления серверами и обхода DPI (DPI bypass) через zapret.")
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
                onClicked: App.openUrl("https://t.me/lumenkvn")
            }
            AccentButton {
                kind: "ghost"
                glyph: "\uE734"  // Shield
                text: I18n.t("Рекомендуемый VPN")
                onClicked: App.openUrl("https://t.me/zapretvpns_bot")
            }
            Item { Layout.fillWidth: true }
        }

        Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
    }
}
