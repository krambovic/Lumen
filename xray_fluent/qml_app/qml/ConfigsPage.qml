import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Layouts
import App 1.0
import "."

Item {
    id: page

    property string core: "singbox"

    // ---- live per-core editor state -------------------------------------
    property var cache: ({})          // core -> snapshot (preserves unsaved edits)
    property bool loaded: false
    property string fileLabel: "--"
    property string templateLabel: "--"
    property bool hasTemplate: false
    property var configItems: []
    property var templateItems: []
    property string activeConfig: ""
    property string activeTemplate: ""
    property string statusLevel: ""
    property string statusMessage: ""
    property string savedText: ""
    readonly property bool dirty: editor.text !== page.savedText

    property var _pendingAction: null

    // ---- reusable styled controls (Windows 11 Fluent look) --------------
    component StyledCombo: FluentCombo {}

    function _indexOfValue(items, value) {
        for (var i = 0; i < items.length; i++)
            if (items[i].value === value)
                return i
        return 0
    }

    function _labels(items) {
        var out = []
        for (var i = 0; i < items.length; i++)
            out.push(items[i].label)
        return out
    }

    function _statusPrefix(level) {
        return level === "success" ? "OK"
             : level === "warning" ? I18n.t("Внимание")
             : level === "error" ? I18n.t("Ошибка")
             : level === "info" ? I18n.t("Инфо") : ""
    }

    function _syncCombos() {
        configCombo.currentIndex = page.configItems.length > 0
            ? _indexOfValue(page.configItems, page.activeConfig) : 0
        templateCombo.currentIndex = page.templateItems.length > 0
            ? _indexOfValue(page.templateItems, page.activeTemplate) : 0
    }

    // Full editor state coming from build_state() on the bridge.
    function _ingest(st) {
        if (!st)
            return
        page.fileLabel = (st.fileLabel && st.fileLabel.length) ? st.fileLabel : "--"
        page.templateLabel = (st.templateLabel && st.templateLabel.length) ? st.templateLabel : "--"
        page.hasTemplate = st.hasTemplate === true
        page.configItems = st.configItems || []
        page.templateItems = st.templateItems || []
        page.activeConfig = st.activeConfig || ""
        page.activeTemplate = st.activeTemplate || ""
        page.statusLevel = st.statusLevel || ""
        page.statusMessage = st.statusMessage || ""
        editor.text = st.text || ""
        page.savedText = editor.text
        page.loaded = true
        _syncCombos()
    }

    // Lighter status-only result (validate / apply).
    function _ingestStatus(st) {
        if (!st)
            return
        page.statusLevel = st.statusLevel || ""
        page.statusMessage = st.statusMessage || ""
        if (st.fileLabel && st.fileLabel.length)
            page.fileLabel = st.fileLabel
    }

    function _snapshot() {
        return {
            "text": editor.text, "savedText": page.savedText,
            "statusLevel": page.statusLevel, "statusMessage": page.statusMessage,
            "fileLabel": page.fileLabel, "templateLabel": page.templateLabel,
            "hasTemplate": page.hasTemplate, "configItems": page.configItems,
            "templateItems": page.templateItems, "activeConfig": page.activeConfig,
            "activeTemplate": page.activeTemplate, "loaded": page.loaded
        }
    }

    function _applySnapshot(s) {
        page.fileLabel = s.fileLabel
        page.templateLabel = s.templateLabel
        page.hasTemplate = s.hasTemplate
        page.configItems = s.configItems
        page.templateItems = s.templateItems
        page.activeConfig = s.activeConfig
        page.activeTemplate = s.activeTemplate
        page.statusLevel = s.statusLevel
        page.statusMessage = s.statusMessage
        page.savedText = s.savedText
        page.loaded = s.loaded
        editor.text = s.text
        _syncCombos()
    }

    function loadCore(c) {
        _ingest(App.loadConfig(c))
    }

    function switchCore(c) {
        if (c === page.core)
            return
        page.cache[page.core] = _snapshot()
        page.core = c
        if (page.cache[c] !== undefined) {
            _applySnapshot(page.cache[c])
            if (!page.loaded)
                loadCore(c)
        } else {
            loadCore(c)
        }
    }

    // ---- discard confirmation -------------------------------------------
    function _guard(action) {
        if (page.dirty) {
            page._pendingAction = action
            confirmOverlay.visible = true
        } else {
            action()
        }
    }
    function _confirmYes() {
        confirmOverlay.visible = false
        var a = page._pendingAction
        page._pendingAction = null
        if (a)
            a()
    }
    function _confirmNo() {
        confirmOverlay.visible = false
        page._pendingAction = null
    }

    // ---- actions ---------------------------------------------------------
    function doSelectConfig(index) {
        var item = page.configItems[index]
        if (!item) { _syncCombos(); return }
        _guard(function() { _ingest(App.selectConfig(page.core, item.value)) })
    }
    function doSelectTemplate(index) {
        var item = page.templateItems[index]
        if (!item) { _syncCombos(); return }
        _guard(function() { _ingest(App.selectTemplate(page.core, item.value)) })
    }
    function doImportTemplate() {
        _guard(function() {
            var st = App.importTemplate(page.core)
            if (st && st.cancelled === true)
                return
            _ingest(st)
        })
    }
    function doReset() {
        if (!page.hasTemplate)
            return
        _guard(function() { _ingest(App.resetConfig(page.core)) })
    }
    function doSave() {
        _ingest(App.saveConfig(page.core, editor.text))
    }
    function doValidate() {
        _ingestStatus(App.validateConfig(page.core, editor.text))
    }
    function doApply() {
        var st = App.applyConfig(page.core, editor.text)
        _ingestStatus(st)
        if (st && st.statusLevel !== "error")
            page.savedText = editor.text
    }

    Component.onCompleted: if (!page.loaded) loadCore(page.core)

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacingLarge

        Text {
            text: I18n.t("Конфиги")
            color: Theme.text; font.family: Theme.fontFamily
            font.pixelSize: Theme.fontTitle; font.weight: Font.DemiBold
        }

        // ---- segmented core selector (sing-box / xray) ----
        Row {
            spacing: 0
            Layout.preferredHeight: 34
            Repeater {
                model: [{ key: "singbox", label: "sing-box" }, { key: "xray", label: "xray" }]
                delegate: Rectangle {
                    width: 120; height: 34
                    readonly property bool active: page.core === modelData.key
                    radius: Theme.radiusSmall
                    color: "transparent"
                    Rectangle {
                        anchors.fill: parent
                        radius: parent.radius
                        color: active ? Theme.accentSoft : Theme.navHover
                        opacity: active || coreTabMouse.containsMouse ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.animations ? 140 : 0 } }
                    }
                    Text {
                        anchors.centerIn: parent
                        text: modelData.label
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal
                        font.weight: active ? Font.DemiBold : Font.Normal
                        color: active ? Theme.text : Theme.textMuted
                    }
                    // Fluent SegmentedWidget underline indicator
                    Rectangle {
                        anchors.bottom: parent.bottom
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: active ? 24 : 0
                        height: 3; radius: 1.5
                        color: Theme.accent
                        Behavior on width { NumberAnimation { duration: 150; easing.type: Easing.OutCubic } }
                    }
                    MouseArea {
                        id: coreTabMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: page.switchCore(modelData.key)
                    }
                }
            }
        }

        // ---- selectors row ----
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Text { text: I18n.t("Конфиг"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
            StyledCombo {
                id: configCombo
                enabled: page.configItems.length > 0
                Layout.fillWidth: true; Layout.preferredWidth: 280
                model: page.configItems.length > 0 ? page._labels(page.configItems) : [I18n.t("Нет конфигов")]
                onActivated: page.doSelectConfig(configCombo.currentIndex)
            }
            Item { Layout.preferredWidth: 12 }
            Text { text: I18n.t("Шаблон"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontNormal }
            StyledCombo {
                id: templateCombo
                enabled: page.templateItems.length > 0
                Layout.fillWidth: true; Layout.preferredWidth: 280
                model: page.templateItems.length > 0 ? page._labels(page.templateItems) : [I18n.t("Шаблон не выбран")]
                onActivated: page.doSelectTemplate(templateCombo.currentIndex)
            }
            AccentButton {
                kind: "ghost"
                glyph: "\uE838"
                iconOnly: true
                tip: I18n.t("Открыть папку конфигов")
                onClicked: App.openConfigDirectory(page.core)
            }
        }

        // ---- toolbar ----
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            AccentButton { kind: "ghost"; text: I18n.t("Импорт шаблона"); enabled: page.loaded; onClicked: page.doImportTemplate() }
            AccentButton { kind: "ghost"; text: I18n.t("Сбросить к шаблону"); enabled: page.loaded && page.hasTemplate; onClicked: page.doReset() }
            AccentButton { kind: "ghost"; text: I18n.t("Сохранить"); enabled: page.loaded; onClicked: page.doSave() }
            AccentButton { kind: "ghost"; text: I18n.t("Проверить JSON"); enabled: page.loaded; onClicked: page.doValidate() }
            Item { Layout.fillWidth: true }
            AccentButton { kind: "accent"; text: I18n.t("Применить"); enabled: page.loaded; onClicked: page.doApply() }
        }

        // ---- editor ----
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: Theme.radiusSmall
            color: Theme.card
            border.width: 1; border.color: Theme.borderSolid
            Flickable {
                id: editorFlick
                anchors.fill: parent
                anchors.margins: 1
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                flickDeceleration: 5000
                maximumFlickVelocity: 3000
                pixelAligned: true
                // eased mouse-wheel scroll (matches FluentScroll feel)
                property real wheelStep: 92
                NumberAnimation {
                    id: editorScrollAnim
                    target: editorFlick; property: "contentY"
                    duration: 320; easing.type: Easing.OutQuint
                }
                WheelHandler {
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                    onWheel: (ev) => {
                        var maxY = Math.max(0, editorFlick.contentHeight - editorFlick.height)
                        if (maxY <= 0) { ev.accepted = true; return }
                        if (ev.pixelDelta.y !== 0) {
                            editorScrollAnim.stop()
                            editorFlick.contentY = Math.max(0, Math.min(maxY, editorFlick.contentY - ev.pixelDelta.y))
                        } else {
                            var base = editorScrollAnim.running ? editorScrollAnim.to : editorFlick.contentY
                            var target = Math.max(0, Math.min(maxY, base - (ev.angleDelta.y / 120) * editorFlick.wheelStep))
                            editorScrollAnim.to = target
                            editorScrollAnim.restart()
                        }
                        editorVbar.flash()
                        ev.accepted = true
                    }
                }

                TextArea.flickable: FluentTextArea {
                    id: editor
                    placeholderText: "Raw " + (page.core === "singbox" ? "sing-box" : "xray") + ".json"
                    color: Theme.text
                    font.family: "Consolas"
                    font.pixelSize: 13
                    wrapMode: TextArea.NoWrap
                    background: null
                    selectByMouse: true
                }

                ScrollBar.vertical: FluentScrollBar {
                    id: editorVbar
                    anchors.top: parent.top
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                }
                ScrollBar.horizontal: FluentScrollBar {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                }
            }
        }

    }

    // ---- discard-confirmation modal overlay -----------------------------
    Rectangle {
        id: confirmOverlay
        anchors.fill: parent
        visible: false
        z: 100
        color: "#80000000"
        MouseArea { anchors.fill: parent; onClicked: {} }   // swallow clicks
        Rectangle {
            anchors.centerIn: parent
            width: Math.min(420, parent.width - 80)
            height: contentCol.implicitHeight + 40
            radius: Theme.radius
            color: Theme.flyout
            border.width: 1; border.color: Theme.flyoutBorder
            ColumnLayout {
                id: contentCol
                anchors.fill: parent
                anchors.margins: 20
                spacing: 16
                Text {
                    text: I18n.t("Несохранённые изменения")
                    color: Theme.text; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontStrong; font.weight: Font.DemiBold
                    Layout.fillWidth: true; wrapMode: Text.WordWrap
                }
                Text {
                    text: I18n.t("В текущем редакторе есть несохранённые изменения. Отменить их и продолжить?")
                    color: Theme.textMuted; font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontNormal
                    Layout.fillWidth: true; wrapMode: Text.WordWrap
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Item { Layout.fillWidth: true }
                    AccentButton { kind: "ghost"; text: I18n.t("Отмена"); onClicked: page._confirmNo() }
                    AccentButton { kind: "danger"; text: I18n.t("Отменить изменения"); onClicked: page._confirmYes() }
                }
            }
        }
    }
}
