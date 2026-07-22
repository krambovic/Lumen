import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

// Protocol-aware server editor. Python supplies the exact field schema for
// the selected protocol; the dialog never exposes keys unsupported by it.
Popup {
    id: dlg
    modal: true
    dim: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape

    parent: Overlay.overlay
    anchors.centerIn: Overlay.overlay
    width: Math.min(760, (Overlay.overlay ? Overlay.overlay.width : 760) - 40)
    height: Math.min(720, (Overlay.overlay ? Overlay.overlay.height : 720) - 48)

    property string nodeId: ""
    property bool creating: false
    property string protocolKey: ""
    property var protocolFields: []
    property var fieldValues: ({})
    property int fieldRevision: 0
    signal nodeCreated(string nodeId)

    background: Rectangle {
        color: Theme.flyout
        radius: 10
        border.width: 1
        border.color: Theme.flyoutBorder
    }

    Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.4) }

    function fieldValue(key, fallback) {
        var revision = fieldRevision
        var value = fieldValues[key]
        return value === undefined || value === null ? (fallback === undefined ? "" : fallback) : value
    }

    function setFieldValue(key, value) {
        fieldValues[key] = value
        fieldRevision += 1
    }

    function applyWireGuardAmneziaDefaults() {
        if (protocolKey !== "wireguard")
            return
        var defaults = {
            "awg_jc": 4,
            "awg_jmin": 40,
            "awg_jmax": 70,
            "awg_s1": 0,
            "awg_s2": 0,
            "awg_s3": 0,
            "awg_s4": 0,
            "awg_h1": "1",
            "awg_h2": "2",
            "awg_h3": "3",
            "awg_h4": "4"
        }
        for (var key in defaults) {
            var current = fieldValue(key, "")
            if (current === "" || current === null || current === undefined)
                fieldValues[key] = defaults[key]
        }
        fieldRevision += 1
    }

    function fieldVisible(spec) {
        var revision = fieldRevision
        if (!spec || !spec.whenKey || !spec.whenValues)
            return true
        var current = String(fieldValue(spec.whenKey, ""))
        return spec.whenValues.indexOf(current) >= 0
    }

    function openFor(id) {
        creating = false
        nodeId = id
        var data = App.nodeEditFields(id) || {}
        protocolKey = data.protocolKey || ""
        nameField.text = data.name || ""
        groupField.text = data.group || ""
        protocolCombo.currentIndex = Math.max(0, App.manualNodeProtocols.indexOf(protocolKey))
        editHint.text = data.editHint || ""
        protocolFields = data.protocolFields || []
        var values = {}
        for (var i = 0; i < protocolFields.length; ++i) {
            var spec = protocolFields[i]
            values[spec.key] = spec.value
        }
        fieldValues = values
        fieldRevision += 1
        open()
    }

    function openNew(groupName) {
        creating = true
        nodeId = ""
        nameField.text = ""
        groupField.text = groupName || "Default"
        loadNewProtocol("vless")
        open()
    }

    function loadNewProtocol(protocol) {
        protocolKey = protocol || "vless"
        var data = App.manualNodeFields(protocolKey, groupField.text) || {}
        protocolFields = data.protocolFields || []
        editHint.text = data.editHint || ""
        var values = {}
        for (var i = 0; i < protocolFields.length; ++i) {
            var spec = protocolFields[i]
            values[spec.key] = spec.value
        }
        fieldValues = values
        fieldRevision += 1
        protocolCombo.currentIndex = Math.max(0, App.manualNodeProtocols.indexOf(protocolKey))
    }

    function saveChanges() {
        var payload = {
            "name": nameField.text,
            "group": groupField.text,
            "protocol": protocolKey
        }
        for (var i = 0; i < protocolFields.length; ++i) {
            var key = protocolFields[i].key
            payload[key] = fieldValue(key, "")
        }
        if (creating) {
            var createdId = App.createManualNode(payload)
            if (!createdId)
                return
            nodeCreated(createdId)
        } else {
            App.saveNodeEdit(nodeId, payload)
        }
        close()
    }

    component FLabel: Text {
        Layout.alignment: Qt.AlignVCenter
        Layout.preferredWidth: 185
        color: Theme.textMuted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        wrapMode: Text.WordWrap
    }

    component FField: FluentTextField {
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
        background: Rectangle {
            radius: Theme.radiusSmall
            color: Theme.controlFill
            border.width: 1
            border.color: parent.activeFocus ? Theme.accent : Theme.borderSolid
        }
    }

    component DynamicTextField: FField {
        property var fieldSpec: ({})
        text: String(dlg.fieldValue(fieldSpec.key, ""))
        placeholderText: fieldSpec.placeholder || ""
        // Редактор серверов всегда показывает исходное значение целиком:
        // пользователь должен видеть и иметь возможность исправить любой параметр.
        echoMode: TextInput.Normal
        inputMethodHints: fieldSpec.kind === "number" ? Qt.ImhDigitsOnly : Qt.ImhNone
        onTextEdited: dlg.setFieldValue(fieldSpec.key, text)
    }

    component DynamicCombo: FluentCombo {
        property var fieldSpec: ({})
        Layout.fillWidth: true
        model: fieldSpec.options || []

        function syncCurrentValue() {
            var index = model.indexOf(String(dlg.fieldValue(fieldSpec.key, "")))
            currentIndex = index >= 0 ? index : 0
            if (currentIndex >= 0)
                dlg.setFieldValue(fieldSpec.key, currentText)
        }

        Component.onCompleted: Qt.callLater(syncCurrentValue)
        onFieldSpecChanged: Qt.callLater(syncCurrentValue)
        onActivated: dlg.setFieldValue(fieldSpec.key, currentText)
    }

    component DynamicSwitchHost: Item {
        property var fieldSpec: ({})
        implicitHeight: Theme.controlHeight

        Switch {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            Universal.theme: Theme.dark ? Universal.Dark : Universal.Light
            Universal.accent: Theme.accent
            checked: dlg.fieldValue(parent.fieldSpec.key, false) === true
            onToggled: {
                dlg.setFieldValue(parent.fieldSpec.key, checked)
                if (parent.fieldSpec.key === "amneziaEnabled" && checked)
                    dlg.applyWireGuardAmneziaDefaults()
            }
        }
    }

    component ProtocolFieldRow: RowLayout {
        property var fieldSpec: ({})
        Layout.fillWidth: true
        spacing: 14
        visible: dlg.fieldVisible(fieldSpec)

        FLabel { text: I18n.t(parent.fieldSpec.label || parent.fieldSpec.key) }
        Loader {
            id: editorLoader
            Layout.fillWidth: true
            property var spec: parent.fieldSpec
            sourceComponent: spec.kind === "combo" ? comboEditor
                             : spec.kind === "bool" ? switchEditor
                             : textEditor
            onLoaded: item.fieldSpec = spec
        }
    }

    Component { id: textEditor; DynamicTextField {} }
    Component { id: comboEditor; DynamicCombo {} }
    Component { id: switchEditor; DynamicSwitchHost {} }

    contentItem: ColumnLayout {
        spacing: 0

        Text {
            Layout.fillWidth: true
            Layout.leftMargin: 20
            Layout.rightMargin: 20
            Layout.topMargin: 18
            Layout.bottomMargin: 12
            text: I18n.t(dlg.creating ? "Новый сервер" : "Редактирование сервера")
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontTitle
            font.bold: true
        }

        ScrollView {
            id: formScroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: 20
            Layout.rightMargin: 14
            clip: true
            contentWidth: availableWidth
            contentHeight: formColumn.implicitHeight
            Component.onCompleted: {
                if (contentItem)
                    contentItem.boundsBehavior = Flickable.StopAtBounds
            }
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical: FluentScrollBar {
                policy: ScrollBar.AsNeeded
                interactive: true
                anchors.top: parent.top
                anchors.right: parent.right
                anchors.bottom: parent.bottom
            }

            ColumnLayout {
                id: formColumn
                width: formScroll.availableWidth
                spacing: 10

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 14
                    FLabel { text: I18n.t("Псевдоним") }
                    FField { id: nameField }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 14
                    FLabel { text: I18n.t("Группа") }
                    FField { id: groupField }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 14
                    FLabel { text: I18n.t("Протокол") }
                    FluentCombo {
                        id: protocolCombo
                        Layout.fillWidth: true
                        model: App.manualNodeProtocols
                        visible: dlg.creating
                        onActivated: {
                            if (dlg.creating)
                                dlg.loadNewProtocol(currentText)
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Theme.controlHeight
                        Layout.leftMargin: 10
                        visible: !dlg.creating
                        text: dlg.protocolKey.toUpperCase() || "?"
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontNormal
                        verticalAlignment: Text.AlignVCenter
                    }
                }

                Text {
                    id: editHint
                    Layout.fillWidth: true
                    visible: text.length > 0
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    wrapMode: Text.WordWrap
                }

                Rectangle {
                    Layout.fillWidth: true
                    visible: protocolFields.length > 0
                    implicitHeight: 1
                    color: Theme.divider
                }

                Repeater {
                    model: dlg.protocolFields
                    delegate: ProtocolFieldRow { fieldSpec: modelData }
                }
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
                onClicked: dlg.saveChanges()
            }
        }
    }
}
