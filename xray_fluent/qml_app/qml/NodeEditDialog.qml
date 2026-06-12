import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import App 1.0
import "."

// «Редактирование сервера» — QML twin of ui/node_edit_dialog.py.
//
// The form only collects values; the protocol-specific outbound load/rebuild
// lives in Python (bridge/node_edit_helpers.py) so the behaviour is identical
// to the classic qfluentwidgets dialog. openFor(nodeId) pulls the flattened
// fields from App.nodeEditFields and Save pushes them back via App.saveNodeEdit.
Popup {
    id: dlg
    modal: true
    dim: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape

    parent: Overlay.overlay
    anchors.centerIn: Overlay.overlay
    width: 660
    height: Math.min(640, (Overlay.overlay ? Overlay.overlay.height : 640) - 48)

    property string nodeId: ""
    property var opts: App.nodeEditOptions

    readonly property string secVal: secCombo.currentText
    readonly property bool isTls: secVal === "tls" || secVal === "reality"
    readonly property bool isReality: secVal === "reality"
    readonly property bool isRaw: networkCombo.currentText === "raw"

    background: Rectangle {
        color: Theme.flyout
        radius: 10
        border.width: 1
        border.color: Theme.flyoutBorder
    }

    Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.4) }

    function _setCombo(combo, model, val) {
        var i = (model && model.indexOf) ? model.indexOf(val) : -1
        combo.currentIndex = i >= 0 ? i : 0
    }

    function openFor(id) {
        nodeId = id
        var f = App.nodeEditFields(id) || {}
        nameField.text = f.name || ""
        groupField.text = f.group || ""
        tagsField.text = f.tags || ""
        addressField.text = f.server || ""
        portField.text = f.port || ""
        protoLabel.text = f.protocol || "?"
        uuidField.text = f.uuid || ""
        encField.text = f.encryption || ""
        sniField.text = f.sni || ""
        fpField.text = f.fingerprint || ""
        pbkField.text = f.publicKey || ""
        sidField.text = f.shortId || ""
        spxField.text = f.spiderX || ""
        pqvField.text = f.pqv || ""
        fmField.text = f.finalmask || ""
        _setCombo(flowCombo, opts.flows, f.flow || "")
        _setCombo(networkCombo, opts.networks, f.network || "tcp")
        _setCombo(rawCombo, opts.rawHeaders, f.rawHeader || "none")
        _setCombo(secCombo, opts.security, f.security || "none")
        open()
    }

    function _save() {
        App.saveNodeEdit(nodeId, {
            "name": nameField.text,
            "group": groupField.text,
            "tags": tagsField.text,
            "server": addressField.text,
            "port": portField.text,
            "uuid": uuidField.text,
            "encryption": encField.text,
            "flow": flowCombo.currentText,
            "network": networkCombo.currentText,
            "rawHeader": rawCombo.currentText,
            "security": secCombo.currentText,
            "sni": sniField.text,
            "fingerprint": fpField.text,
            "publicKey": pbkField.text,
            "shortId": sidField.text,
            "spiderX": spxField.text,
            "pqv": pqvField.text,
            "finalmask": fmField.text
        })
        close()
    }

    // ── reusable form atoms ───────────────────────────────
    component FLabel: Text {
        Layout.alignment: Qt.AlignVCenter
        Layout.preferredWidth: 130
        color: Theme.textMuted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
    component FField: TextField {
        Layout.fillWidth: true
        implicitHeight: Theme.controlHeight
        leftPadding: 10
        rightPadding: 10
        topPadding: 0
        bottomPadding: 0
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontNormal
        selectByMouse: true
        verticalAlignment: TextInput.AlignVCenter
        opacity: enabled ? 1.0 : 0.5
        background: Rectangle {
            radius: Theme.radiusSmall
            color: Theme.controlFill
            border.width: 1
            border.color: parent.activeFocus ? Theme.accent : Theme.borderSolid
        }
    }

    contentItem: ColumnLayout {
        spacing: 0

        Text {
            Layout.fillWidth: true
            Layout.margins: 20
            Layout.bottomMargin: 6
            text: I18n.t("Редактирование сервера")
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontTitle
            font.bold: true
        }

        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: 20
            Layout.rightMargin: 14
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

            GridLayout {
                width: dlg.width - 40
                columns: 2
                columnSpacing: 14
                rowSpacing: 10

                FLabel { text: I18n.t("Псевдоним") }
                FField { id: nameField }

                FLabel { text: I18n.t("Группа") }
                FField { id: groupField }

                FLabel { text: I18n.t("Теги") }
                FField { id: tagsField; placeholderText: I18n.t("тег1, тег2") }

                FLabel { text: I18n.t("Адрес") }
                FField { id: addressField }

                FLabel { text: I18n.t("Порт") }
                FField { id: portField; inputMethodHints: Qt.ImhDigitsOnly }

                FLabel { text: I18n.t("Протокол") }
                Text {
                    id: protoLabel
                    Layout.fillWidth: true
                    text: "?"
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontNormal
                    verticalAlignment: Text.AlignVCenter
                }

                FLabel { text: "UUID / id" }
                FField { id: uuidField }

                FLabel { text: "Flow" }
                FluentCombo { id: flowCombo; Layout.fillWidth: true; model: dlg.opts.flows }

                FLabel { text: I18n.t("Шифрование") }
                FField { id: encField }

                FLabel { text: I18n.t("Транспорт") }
                FluentCombo { id: networkCombo; Layout.fillWidth: true; model: dlg.opts.networks }

                FLabel { text: "Raw camouflage" }
                FluentCombo { id: rawCombo; Layout.fillWidth: true; model: dlg.opts.rawHeaders; enabled: dlg.isRaw }

                FLabel { text: "TLS" }
                FluentCombo { id: secCombo; Layout.fillWidth: true; model: dlg.opts.security }

                FLabel { text: "SNI" }
                FField { id: sniField; enabled: dlg.isTls }

                FLabel { text: "Fingerprint" }
                FField { id: fpField; enabled: dlg.isTls; placeholderText: "chrome, firefox, …" }

                FLabel { text: "Public key" }
                FField { id: pbkField; enabled: dlg.isReality }

                FLabel { text: "ShortId" }
                FField { id: sidField; enabled: dlg.isReality }

                FLabel { text: "SpiderX" }
                FField { id: spxField; enabled: dlg.isReality }

                FLabel { text: "Mldsa65Verify" }
                FField { id: pqvField; enabled: dlg.isReality }

                FLabel { text: "Finalmask" }
                FField { id: fmField }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 20
            Layout.topMargin: 12
            spacing: 10
            Item { Layout.fillWidth: true }
            AccentButton {
                kind: "ghost"
                text: I18n.t("Отмена")
                onClicked: dlg.close()
            }
            AccentButton {
                kind: "accent"
                text: I18n.t("Сохранить")
                onClicked: dlg._save()
            }
        }
    }
}
