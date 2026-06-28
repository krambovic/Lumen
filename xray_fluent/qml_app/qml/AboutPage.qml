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
            text: I18n.t("Lumen KVN — уникальный туннель до любой страны мира, созданный передовыми мировыми инженерами (не является тем чем вы думаете).")
        }
        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.textMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontNormal
            text: I18n.t("Позволяет ускорить замедленные сервера Ютуба и Дискорда в случае если те перестали работать и начали деградировать.")
        }
        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.textMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontNormal
            text: I18n.t("Также подходит для ускорения игровых серверов.")
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
                glyph: "\uE7BF"  // Shopping cart
                text: I18n.t("Купить подписку")
                onClicked: App.openUrl("https://t.me/zapretvpns_bot")
            }
            Item { Layout.fillWidth: true }
        }

        // ── Footer ──
        Text {
            Layout.topMargin: 4
            text: I18n.t("Подробнее здесь @zapretvpns_bot")
            color: Theme.textFaint
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }

        Item { Layout.fillHeight: true; Layout.preferredHeight: 1 }
    }
}
