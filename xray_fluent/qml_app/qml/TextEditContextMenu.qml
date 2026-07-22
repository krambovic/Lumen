import QtQuick
import QtQuick.Controls.Universal
import "."

Item {
    id: root
    property var editor: parent
    property real requestedMenuX: 0
    property real requestedMenuY: 0
    anchors.fill: parent
    z: 1000

    function openAt(menuX, menuY) {
        requestedMenuX = menuX
        requestedMenuY = menuY
        editMenuLoader.active = true
        Qt.callLater(function() {
            var menu = editMenuLoader.item
            if (!menu) return
            if (menu.opened)
                menu.close()
            Qt.callLater(function() {
                if (!editMenuLoader.item) return
                editMenuLoader.item.x = root.requestedMenuX
                editMenuLoader.item.y = root.requestedMenuY
                editMenuLoader.item.open()
            })
        })
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        hoverEnabled: true
        cursorShape: Qt.IBeamCursor
        preventStealing: true
        onPressed: (mouse) => {
            mouse.accepted = true
            root.editor.forceActiveFocus()
            root.openAt(mouse.x, mouse.y)
        }
    }

    // Text editors are used heavily in the log delegate.  Instantiate the
    // context menu only on the first right click instead of building nine menu
    // items for every visible line.
    Loader {
        id: editMenuLoader
        active: false
        sourceComponent: Component {
            FluentMenu {
                parent: root
                width: 190
                // Consume clicks while the menu is open.  Otherwise a second
                // right click can leak through Qt's popup overlay to the
                // underlying TextInput and summon the platform context menu.
                modal: true
                dim: false
                closePolicy: Popup.CloseOnEscape

                // A transparent modal catcher prevents the platform TextInput
                // menu from receiving the same second right-click.  Right-click
                // inside this editor moves our menu; any other outside click
                // simply closes it.  Unlike a dialog this never shades the page.
                Overlay.modal: MouseArea {
                    id: menuOverlayCatcher
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onPressed: (mouse) => {
                        mouse.accepted = true
                        var point = root.mapFromItem(menuOverlayCatcher, mouse.x, mouse.y)
                        var insideEditor = point.x >= 0 && point.y >= 0
                                && point.x <= root.width && point.y <= root.height
                        editMenuLoader.item.close()
                        if (mouse.button === Qt.RightButton && insideEditor) {
                            root.editor.forceActiveFocus()
                            root.openAt(point.x, point.y)
                        }
                    }
                }

                FluentMenuItem {
                    text: I18n.t("Отменить")
                    enabled: !!root.editor && root.editor.canUndo
                    onTriggered: root.editor.undo()
                }
                FluentMenuItem {
                    text: I18n.t("Повторить")
                    enabled: !!root.editor && root.editor.canRedo
                    onTriggered: root.editor.redo()
                }
                FluentMenuSeparator {}
                FluentMenuItem {
                    text: I18n.t("Вырезать")
                    enabled: !!root.editor && !root.editor.readOnly && root.editor.selectedText.length > 0
                    onTriggered: root.editor.cut()
                }
                FluentMenuItem {
                    text: I18n.t("Копировать")
                    enabled: !!root.editor && root.editor.selectedText.length > 0
                    onTriggered: root.editor.copy()
                }
                FluentMenuItem {
                    text: I18n.t("Вставить")
                    enabled: !!root.editor && !root.editor.readOnly && root.editor.canPaste
                    onTriggered: root.editor.paste()
                }
                FluentMenuItem {
                    text: I18n.t("Удалить")
                    enabled: !!root.editor && !root.editor.readOnly && root.editor.selectedText.length > 0
                    onTriggered: root.editor.remove(root.editor.selectionStart, root.editor.selectionEnd)
                }
                FluentMenuSeparator {}
                FluentMenuItem {
                    text: I18n.t("Выделить всё")
                    enabled: !!root.editor && root.editor.length > 0
                    onTriggered: root.editor.selectAll()
                }
            }
        }
    }
}
