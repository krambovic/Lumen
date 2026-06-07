import QtQuick
import QtQuick.Controls.Universal
import "."

// Windows 11 / WinUI-style ComboBox shared by every page.
//
// The built-in Universal ComboBox pop-up looks like a square Windows-8 "Metro"
// flyout (sharp corners, full-bleed highlight). This component replaces the
// background, content, indicator, item delegate and pop-up so the closed field
// and the drop-down both match the rounded, padded Fluent look of the original
// qfluentwidgets ComboBox:
//   * rounded field with a hairline border that turns accent on focus
//   * chevron indicator
//   * rounded, padded flyout with an opaque Theme.flyout fill
//   * each row is an inset rounded pill that highlights on hover, and the
//     current item gets a short accent bar on its leading edge
ComboBox {
    id: control

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontNormal
    implicitHeight: Theme.controlHeight
    leftPadding: 12
    rightPadding: 34
    topPadding: 0
    bottomPadding: 0
    opacity: enabled ? 1.0 : 0.5

    contentItem: Text {
        text: control.displayText
        font: control.font
        color: Theme.text
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    indicator: Text {
        x: control.width - width - 12
        y: control.topPadding + (control.availableHeight - height) / 2
        text: "\uE70D" // ChevronDown
        font.family: "Segoe Fluent Icons"
        font.pixelSize: 12
        color: Theme.textMuted
    }

    background: Rectangle {
        implicitWidth: 120
        radius: Theme.radiusSmall
        color: control.pressed ? Theme.controlFillPressed
               : (control.hovered ? Theme.controlFillHover : Theme.controlFill)
        border.width: 1
        border.color: control.activeFocus || control.popup.visible ? Theme.accent : Theme.borderSolid
        Behavior on color { ColorAnimation { duration: 110 } }
    }

    delegate: ItemDelegate {
        id: item
        width: ListView.view ? ListView.view.width : control.width
        height: 34
        padding: 0
        highlighted: control.highlightedIndex === index

        contentItem: Text {
            leftPadding: 12
            rightPadding: 12
            text: (control.textRole && typeof modelData === "object" && modelData !== null)
                  ? modelData[control.textRole]
                  : modelData
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
            color: item.highlighted ? Theme.cardHover : "transparent"
            Rectangle {
                visible: control.currentIndex === index
                width: 3
                radius: 1.5
                height: parent.height * 0.5
                anchors.left: parent.left
                anchors.leftMargin: 1
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.accent
            }
        }
    }

    popup: Popup {
        y: control.height + 4
        width: control.width
        padding: 4
        implicitHeight: Math.min(contentItem.implicitHeight + topPadding + bottomPadding, 320)

        background: Rectangle {
            radius: 8
            color: Theme.flyout
            border.width: 1
            border.color: Theme.flyoutBorder
        }

        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
            boundsBehavior: Flickable.StopAtBounds
            ScrollIndicator.vertical: ScrollIndicator {}
        }

        enter: Transition {
            NumberAnimation { property: "opacity"; from: 0.0; to: 1.0; duration: 90 }
        }
    }
}
