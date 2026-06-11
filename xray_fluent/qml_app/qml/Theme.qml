pragma Singleton
import QtQuick

// Central design tokens, restyled to match the original qfluentwidgets
// (Windows 11 Fluent) look: light theme by default, neutral Mica-style
// backgrounds, white cards with a hairline border, Segoe UI typography and the
// #0078D4 system accent. `dark` and `accent` are driven from Main.qml (which
// resolves the saved theme — light / dark / system — and the accent colour),
// so every consumer updates reactively through these readonly tokens.
QtObject {
    id: theme

    // ---- Driven from Main.qml ------------------------------------------
    property bool dark: false
    property color accent: "#0078D4"

    // ---- Accent derivatives --------------------------------------------
    readonly property color accentHover: dark ? Qt.lighter(accent, 1.12) : Qt.darker(accent, 1.06)
    readonly property color accentPressed: dark ? Qt.lighter(accent, 1.22) : Qt.darker(accent, 1.14)
    readonly property color accentText: "#FFFFFF"
    readonly property color accentSoft: dark ? Qt.rgba(accent.r, accent.g, accent.b, 0.22)
                                             : Qt.rgba(accent.r, accent.g, accent.b, 0.10)

    // ---- Surfaces (Fluent Mica / layer palette) ------------------------
    // Window / Mica base
    readonly property color bg: dark ? "#202020" : "#F3F3F3"
    // Fallback fill painted behind everything in Main.qml. Transparent so the
    // Windows 11 DWM Mica backdrop shows through the gaps between the opaque
    // nav pane and cards. (On systems without Mica the window background falls
    // back to the desktop; the solid panes/cards still render normally.)
    readonly property color micaBase: "transparent"
    // Slightly raised base (nav pane). A faint translucent layer over Mica
    // (Fluent "Mica Alt" pane) instead of a flat solid grey, so the backdrop
    // shows through exactly like the original FluentWindow navigation pane.
    readonly property color bgElevated: dark ? Qt.rgba(1, 1, 1, 0.030) : Qt.rgba(1, 1, 1, 0.55)
    // Card / layer fill = Fluent CardBackgroundFillColorDefault composited over
    // the Mica backdrop. Translucent (not solid #2B2B2B) so cards read as the
    // subtle lighter panels of the original instead of flat opaque grey.
    readonly property color card: dark ? Qt.rgba(1, 1, 1, 0.0512) : Qt.rgba(1, 1, 1, 0.70)
    readonly property color cardHover: dark ? Qt.rgba(1, 1, 1, 0.0837) : Qt.rgba(0, 0, 0, 0.024)
    readonly property color cardPressed: dark ? Qt.rgba(1, 1, 1, 0.0326) : Qt.rgba(0, 0, 0, 0.040)

    // Opaque flyout / menu / popup surface. The Mica tokens above (card,
    // bgElevated) are deliberately TRANSLUCENT so the DWM backdrop shows
    // through panes and cards. Pop-ups that float over page content - context
    // menus, combobox dropdowns, toasts - must instead be fully OPAQUE, or the
    // list/page behind them shows straight through (the "transparent menu"
    // bug). Fluent flyout fill ≈ #2C2C2C (dark) / #FBFBFB (light).
    readonly property color flyout: dark ? "#2C2C2C" : "#FBFBFB"
    readonly property color flyoutBorder: dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0, 0, 0, 0.13)

    // ---- Lines ----------------------------------------------------------
    // Hairline control/card border (Fluent CardStrokeColorDefault)
    readonly property color borderSolid: dark ? Qt.rgba(1, 1, 1, 0.09) : Qt.rgba(0, 0, 0, 0.07)
    // Stronger divider
    readonly property color divider: dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(0, 0, 0, 0.06)
    // The subtle darker bottom edge Fluent cards have
    readonly property color borderBottom: dark ? Qt.rgba(0, 0, 0, 0.20) : Qt.rgba(0, 0, 0, 0.10)

    // ---- Text (Fluent TextFillColor ramp) ------------------------------
    readonly property color text: dark ? Qt.rgba(1, 1, 1, 1.0) : Qt.rgba(0, 0, 0, 0.89)
    readonly property color textMuted: dark ? Qt.rgba(1, 1, 1, 0.79) : Qt.rgba(0, 0, 0, 0.61)
    readonly property color textFaint: dark ? Qt.rgba(1, 1, 1, 0.55) : Qt.rgba(0, 0, 0, 0.45)

    // ---- Semantic colours ----------------------------------------------
    readonly property color success: dark ? "#6CCB5F" : "#0F7B0F"
    readonly property color warning: dark ? "#FCE100" : "#9D5D00"
    readonly property color danger: dark ? "#FF99A4" : "#C42B1C"
    // Solid fill for destructive buttons. `danger` above is tuned for legible
    // error TEXT (a pale pink in dark mode), which is too washed-out behind a
    // white button label, so filled danger buttons use this stronger, more
    // saturated red for proper contrast with the white text in both themes.
    readonly property color dangerFill: dark ? "#D13438" : "#C42B1C"
    readonly property color info: accent

    // ---- Standard (non-accent) button fill -----------------------------
    readonly property color controlFill: dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(1, 1, 1, 0.70)
    readonly property color controlFillHover: dark ? Qt.rgba(1, 1, 1, 0.08) : Qt.rgba(249 / 255, 249 / 255, 249 / 255, 0.5)
    readonly property color controlFillPressed: dark ? Qt.rgba(1, 1, 1, 0.03) : Qt.rgba(249 / 255, 249 / 255, 249 / 255, 0.3)

    // ---- Geometry -------------------------------------------------------
    readonly property int radius: 7          // cards
    readonly property int radiusSmall: 4     // controls (Fluent control corner)
    readonly property int spacing: 12
    readonly property int spacingLarge: 20
    readonly property int railWidth: 250
    readonly property int railWidthCompact: 50
    readonly property int controlHeight: 32

    // ---- Typography (Fluent / Segoe UI) --------------------------------
    readonly property string fontFamily: "Segoe UI"
    readonly property int fontSmall: 12      // CaptionLabel
    readonly property int fontNormal: 14     // BodyLabel
    readonly property int fontStrong: 14     // StrongBodyLabel (bold)
    readonly property int fontTitle: 20      // SubtitleLabel
    readonly property int fontHero: 28       // TitleLabel

    // ---- Helpers --------------------------------------------------------
    function pingColor(ping) {
        if (ping === undefined || ping === null || ping < 0)
            return textFaint;
        if (ping < 120)
            return success;
        if (ping < 250)
            return warning;
        return danger;
    }

    function formatRate(bps) {
        var v = bps;
        if (v === undefined || v === null || v < 0 || !isFinite(v))
            v = 0;
        if (v >= 1024 * 1024 * 1024)
            return (v / (1024 * 1024 * 1024)).toFixed(2) + " \u0413\u0431/\u0441";
        if (v >= 1024 * 1024)
            return (v / (1024 * 1024)).toFixed(2) + " \u041c\u0431/\u0441";
        if (v >= 1024)
            return (v / 1024).toFixed(1) + " \u041a\u0431/\u0441";
        return Math.round(v) + " \u0431/\u0441";
    }
}
