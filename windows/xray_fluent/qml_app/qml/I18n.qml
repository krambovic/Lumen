pragma Singleton
import QtQuick
import App 1.0

QtObject {
    id: i18n

    readonly property string language: App.language

    readonly property var map: App.translations

    function t(msgid, params) {
        if (msgid === undefined || msgid === null)
            return ""
        var src = "" + msgid
        var dict = map
        var text = (dict && dict[src] !== undefined) ? dict[src] : src
        if (params !== undefined && params !== null) {
            text = text.replace(/\{(\w+)\}/g, function(full, name) {
                return (params[name] !== undefined) ? ("" + params[name]) : full
            })
        }
        return text
    }
}
