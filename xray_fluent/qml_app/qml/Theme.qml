pragma Singleton
import QtQuick

// Central design tokens (Windows 11 Fluent look). `dark`/`accent` plus the 2.0
// appearance inputs (density / cornerRadius / fontScale / fontFamilyName /
// animations / backdrop / preset) are driven from Main.qml out of saved
// settings, so every consumer updates reactively through these tokens.
QtObject {
    id: theme

    // ---- Driven from Main.qml ------------------------------------------
    property bool dark: false
    property color accent: "#0078D4"

    // ---- 2.0 appearance inputs (persisted settings) --------------------
    property string density: "comfortable"    // compact | comfortable | spacious
    property int cornerRadius: 7               // card corner, clamped 0..20
    property real fontScale: 1.0               // 0.85..1.30
    property string fontFamilyName: "Segoe UI"
    property bool animations: true             // master motion switch
    property string preset: "default"          // default | midnight | nord | ...
    property string baseTint: ""               // "" = выкл; иначе #RRGGBB — свой базовый тон окна (приоритет над пресетом)
    property bool backdropAvailable: true       // false на Win10 (нет Mica) → непрозрачный фон

    readonly property bool amoled: preset === "midnight"

    // ---- Theme presets (surface palettes) ------------------------------
    readonly property var _presetMap: ({
        "default":   { d0: "#202020", d1: "#2C2C2C", lt: "#F3F3F3", win: "" },
        "midnight":  { d0: "#000000", d1: "#1A1A1A", lt: "#F3F3F3", win: "#000000" },
        "nord":      { d0: "#2E3440", d1: "#3B4252", lt: "#D8DEE9", win: "#2E3440" },
        "solarized": { d0: "#1E2A2E", d1: "#26343A", lt: "#FDF6E3", win: "#1A2529" },
        "dracula":   { d0: "#2E2A3C", d1: "#383350", lt: "#E6E1F2", win: "#2E2A3C" },
        "catppuccin": { d0: "#151521", d1: "#20202F", lt: "#DCE0F5", win: "#151521" }
    })
    readonly property var _pal: _presetMap[preset] !== undefined ? _presetMap[preset] : _presetMap["default"]
    readonly property bool presetColored: preset === "nord" || preset === "solarized" || preset === "dracula" || preset === "catppuccin" || (preset === "midnight" && dark)

    // ---- Accent derivatives --------------------------------------------
    readonly property color accentHover: dark ? Qt.lighter(accent, 1.12) : Qt.darker(accent, 1.06)
    readonly property color accentPressed: dark ? Qt.lighter(accent, 1.22) : Qt.darker(accent, 1.14)
    readonly property color accentText: "#FFFFFF"
    readonly property color accentSoft: dark ? Qt.rgba(accent.r, accent.g, accent.b, 0.22)
                                             : Qt.rgba(accent.r, accent.g, accent.b, 0.10)
    // Curated accent palette for the Appearance Studio swatch grid.
    readonly property var accentSwatches: ["#0078D4", "#0099BC", "#00B294", "#107C10",
        "#744DA9", "#B146C2", "#E3008C", "#C30052", "#E81123", "#F7630C", "#FFB900", "#498205"]

    // ---- Surfaces (Fluent Mica / layer palette) ------------------------
    readonly property color bg: dark ? _pal.d0 : _pal.lt

    function _lightTint(hex) {
        var h = ("" + hex).replace("#", "")
        if (h.length < 6) return hex
        var r = parseInt(h.substr(0, 2), 16)
        var g = parseInt(h.substr(2, 2), 16)
        var b = parseInt(h.substr(4, 2), 16)
        return Qt.rgba((r * 0.20 + 0.80 * 255) / 255,
                       (g * 0.20 + 0.80 * 255) / 255,
                       (b * 0.20 + 0.80 * 255) / 255, 1)
    }
    function _darkTint(hex) {
        var h = ("" + hex).replace("#", "")
        if (h.length < 6) return hex
        var r = parseInt(h.substr(0, 2), 16)
        var g = parseInt(h.substr(2, 2), 16)
        var b = parseInt(h.substr(4, 2), 16)
        var base = 0x20
        return Qt.rgba((r * 0.30 + base * 0.70) / 255,
                       (g * 0.30 + base * 0.70) / 255,
                       (b * 0.30 + base * 0.70) / 255, 1)
    }
    readonly property color windowBase: baseTint !== ""
        ? (dark ? _darkTint(baseTint) : _lightTint(baseTint))
        : (presetColored
            ? (dark ? (_pal.win !== "" ? _pal.win : _pal.d0) : _pal.lt)
            : (backdropAvailable ? "transparent" : (dark ? _pal.d0 : _pal.lt)))
    readonly property color micaBase: "transparent"
    // Slightly raised base (nav pane): a very faint layer so Mica stays visible.
    readonly property color bgElevated: dark ? Qt.rgba(1, 1, 1, 0.016) : Qt.rgba(1, 1, 1, 0.55)
    // Card / layer fill composited over the backdrop (translucent on purpose).
    readonly property color card: dark ? Qt.rgba(1, 1, 1, 0.040) : Qt.rgba(1, 1, 1, 0.70)
    readonly property color cardHover: dark ? Qt.rgba(1, 1, 1, 0.0837) : Qt.rgba(0, 0, 0, 0.024)
    readonly property color cardPressed: dark ? Qt.rgba(1, 1, 1, 0.0326) : Qt.rgba(0, 0, 0, 0.040)
    readonly property color flyout: dark ? _pal.d1 : "#FBFBFB"
    readonly property color flyoutBorder: dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0, 0, 0, 0.13)

    // ---- Lines ----------------------------------------------------------
    readonly property color borderSolid: dark ? Qt.rgba(1, 1, 1, 0.09) : Qt.rgba(0, 0, 0, 0.07)
    readonly property color divider: dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(0, 0, 0, 0.06)
    readonly property color borderBottom: dark ? Qt.rgba(0, 0, 0, 0.20) : Qt.rgba(0, 0, 0, 0.10)

    // ---- Text (Fluent TextFillColor ramp) ------------------------------
    readonly property color text: dark ? Qt.rgba(1, 1, 1, 1.0) : Qt.rgba(0, 0, 0, 0.89)
    readonly property color textMuted: dark ? Qt.rgba(1, 1, 1, 0.79) : Qt.rgba(0, 0, 0, 0.61)
    readonly property color textFaint: dark ? Qt.rgba(1, 1, 1, 0.55) : Qt.rgba(0, 0, 0, 0.45)

    // ---- Semantic colours ----------------------------------------------
    readonly property color success: dark ? "#6CCB5F" : "#3BAE45"
    readonly property color warning: dark ? "#FCE100" : "#F4C900"
    readonly property color danger: dark ? "#FF99A4" : "#B84A4A"
    readonly property color dangerFill: dark ? "#D13438" : "#D65A5A"
    readonly property color info: accent

    // ---- Standard (non-accent) button fill -----------------------------
    readonly property color controlFill: dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(1, 1, 1, 0.70)
    readonly property color controlFillHover: dark ? Qt.rgba(1, 1, 1, 0.08) : Qt.rgba(249 / 255, 249 / 255, 249 / 255, 0.5)
    readonly property color controlFillPressed: dark ? Qt.rgba(1, 1, 1, 0.03) : Qt.rgba(249 / 255, 249 / 255, 249 / 255, 0.3)

    // ---- Elevation (drop shadows via MultiEffect) ----------------------
    readonly property color shadowColor: "#000000"
    readonly property real shadowOpacity: dark ? 0.44 : 0.16
    readonly property real shadowBlur: 0.85
    readonly property int shadowVOffset: 5

    // ---- Geometry (density-aware) --------------------------------------
    readonly property real densityScale: density === "compact" ? 0.85 : density === "spacious" ? 1.18 : 1.0
    readonly property int radius: Math.max(0, Math.min(20, cornerRadius))
    readonly property int radiusSmall: Math.max(2, radius - 3)
    readonly property int spacing: Math.round((density === "compact" ? 8 : density === "spacious" ? 16 : 12) * fontScale)
    readonly property int spacingLarge: Math.round((density === "compact" ? 14 : density === "spacious" ? 26 : 20) * fontScale)
    readonly property int railWidth: Math.round(250 * fontScale)
    readonly property int railWidthCompact: Math.round(50 * fontScale)
    readonly property int controlHeight: Math.round((density === "compact" ? 28 : density === "spacious" ? 38 : 32) * fontScale)

    // ---- Typography (Fluent, user-scalable) ----------------------------
    readonly property string fontFamily: (fontFamilyName && fontFamilyName.length) ? fontFamilyName : "Segoe UI"
    readonly property int fontSmall: Math.round(12 * fontScale)   // CaptionLabel
    readonly property int fontNormal: Math.round(14 * fontScale)  // BodyLabel
    readonly property int fontStrong: Math.round(14 * fontScale)  // StrongBodyLabel
    readonly property int fontTitle: Math.round(20 * fontScale)   // SubtitleLabel
    readonly property int fontHero: Math.round(28 * fontScale)    // TitleLabel

    // ---- Motion tokens (respect the animations switch) -----------------
    readonly property int animFast: animations ? 110 : 0
    readonly property int animNormal: animations ? 190 : 0
    readonly property int animSlow: animations ? 320 : 0
    readonly property int easeStandard: Easing.OutCubic
    readonly property int easeEmphasized: Easing.OutQuint

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