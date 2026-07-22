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
        }
        onReleased: (mouse) => {
            mouse.accepted = true
            root.openAt(mouse.x, mouse.y)
        }
        onClicked: (mouse) => mouse.accepted = true
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
                // Keep the menu modeless so the rest of the page remains
                // usable.  Open it only after the right button is released so
                // that the opening click cannot immediately close the popup.
                modal: false
                dim: false
                closePolicy: Popup.CloseOnPressOutside | Popup.CloseOnEscape

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
