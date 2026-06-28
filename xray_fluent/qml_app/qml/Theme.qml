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
    property string backdrop: "mica"           // mica | acrylic | solid
    property bool backdropAvailable: true       // false на Win10 (нет Mica) → непрозрачный фон

    readonly property bool isTranslucent: backdropAvailable && backdrop !== "solid"
    readonly property bool amoled: preset === "midnight"

    // ---- Theme presets (surface palettes) ------------------------------
    readonly property var _presetMap: ({
        "default":   { d0: "#202020", d1: "#2C2C2C", lt: "#F3F3F3", win: "" },
        "absolutely": { d0: "#2D2D2B", d1: "#21211F", lt: "#F5F3ED", win: "#2D2D2B", elevated: "#21211F", card: "#383835", cardHover: "#42423E", cardPressed: "#31312E", flyout: "#252523", flyoutBorder: "#474744", border: "#42423E", divider: "#383835", text: "#F9F9F7", muted: "#B5B5B0", faint: "#888882", control: "#383835", controlHover: "#42423E", controlPressed: "#31312E", colored: true },
        "ayu":        { d0: "#0B0E14", d1: "#11151C", lt: "#FAFAFA", win: "#0B0E14", elevated: "#101521", card: "#151B26", cardHover: "#1F2937", cardPressed: "#10151E", flyout: "#11151C", flyoutBorder: "#2D3646", border: "#2D3646", divider: "#232B38", text: "#E6E1CF", muted: "#B3B1AD", faint: "#7F8692", control: "#151B26", controlHover: "#1F2937", controlPressed: "#10151E", colored: true },
        "dracula":   {
            d0: "#282A36", d1: "#343746", lt: "#F8F8F2", win: "#282A36",
            elevated: "#2D3040", card: "#343746", cardHover: "#44475A",
            cardPressed: "#303342", flyout: "#282A36", flyoutBorder: "#44475A",
            border: "#44475A", divider: "#3B3E50", text: "#F8F8F2",
            muted: "#D7D2E8", faint: "#A8A6B3", control: "#343746",
            controlHover: "#44475A", controlPressed: "#303342", colored: true
        },
        "catppuccin": {
            d0: "#11111B", d1: "#1E1E2E", lt: "#EFF1F5", win: "#11111B",
            elevated: "#181825", card: "#1E1E2E", cardHover: "#313244",
            cardPressed: "#181825", flyout: "#1E1E2E", flyoutBorder: "#313244",
            border: "#313244", divider: "#313244", text: "#CDD6F4",
            muted: "#A6ADC8", faint: "#7F849C", control: "#1E1E2E",
            controlHover: "#313244", controlPressed: "#181825", colored: true
        },
        "codex":      { d0: "#0F1117", d1: "#181B23", lt: "#F5F5F5", win: "#0F1117", elevated: "#141720", card: "#1D212B", cardHover: "#2B3040", cardPressed: "#171A22", flyout: "#181B23", flyoutBorder: "#2B3040", border: "#2B3040", divider: "#252A36", text: "#F4F6FB", muted: "#AEB6C7", faint: "#778092", control: "#1D212B", controlHover: "#2B3040", controlPressed: "#171A22", colored: true },
        "everforest": { d0: "#1E2326", d1: "#272E33", lt: "#FDF6E3", win: "#1E2326", elevated: "#232A2E", card: "#2E383C", cardHover: "#3A464A", cardPressed: "#252D31", flyout: "#272E33", flyoutBorder: "#414B50", border: "#414B50", divider: "#374145", text: "#D3C6AA", muted: "#A7C080", faint: "#859289", control: "#2E383C", controlHover: "#3A464A", controlPressed: "#252D31", colored: true },
        "github":     { d0: "#0D1117", d1: "#161B22", lt: "#F6F8FA", win: "#0D1117", elevated: "#10161F", card: "#161B22", cardHover: "#21262D", cardPressed: "#151A21", flyout: "#161B22", flyoutBorder: "#30363D", border: "#30363D", divider: "#21262D", text: "#E6EDF3", muted: "#8B949E", faint: "#6E7681", control: "#21262D", controlHover: "#30363D", controlPressed: "#161B22", colored: true },
        "gruvbox":    { d0: "#1D2021", d1: "#282828", lt: "#FBF1C7", win: "#1D2021", elevated: "#242422", card: "#32302F", cardHover: "#3C3836", cardPressed: "#282828", flyout: "#282828", flyoutBorder: "#504945", border: "#504945", divider: "#3C3836", text: "#EBDBB2", muted: "#D5C4A1", faint: "#928374", control: "#32302F", controlHover: "#3C3836", controlPressed: "#282828", colored: true },
        "linear":     { d0: "#08090D", d1: "#11131A", lt: "#F7F7F8", win: "#08090D", elevated: "#0D0F16", card: "#181B25", cardHover: "#222635", cardPressed: "#11131A", flyout: "#11131A", flyoutBorder: "#303443", border: "#303443", divider: "#252938", text: "#F7F8FA", muted: "#A8AFBF", faint: "#737B8C", control: "#181B25", controlHover: "#222635", controlPressed: "#11131A", colored: true },
        "lobster":    { d0: "#1B0D13", d1: "#2A151D", lt: "#FFF1F4", win: "#1B0D13", elevated: "#231018", card: "#351A25", cardHover: "#492434", cardPressed: "#2A151D", flyout: "#2A151D", flyoutBorder: "#5A2D3E", border: "#5A2D3E", divider: "#472333", text: "#FFE8EF", muted: "#F2B8C6", faint: "#B98392", control: "#351A25", controlHover: "#492434", controlPressed: "#2A151D", colored: true },
        "material":   { d0: "#0F111A", d1: "#1A1C25", lt: "#ECEFF1", win: "#0F111A", elevated: "#151823", card: "#202331", cardHover: "#2B3040", cardPressed: "#191C27", flyout: "#1A1C25", flyoutBorder: "#33394A", border: "#33394A", divider: "#293042", text: "#EEFFFF", muted: "#B0BEC5", faint: "#697884", control: "#202331", controlHover: "#2B3040", controlPressed: "#191C27", colored: true },
        "matrix":     { d0: "#000000", d1: "#031006", lt: "#E8FFE8", win: "#000000", elevated: "#020A04", card: "#06160A", cardHover: "#0B2A10", cardPressed: "#031006", flyout: "#031006", flyoutBorder: "#145C22", border: "#145C22", divider: "#0D3515", text: "#D9FFE2", muted: "#7CFF91", faint: "#3FAF52", control: "#06160A", controlHover: "#0B2A10", controlPressed: "#031006", colored: true },
        "midnight":   { d0: "#000000", d1: "#1A1A1A", lt: "#F3F3F3", win: "#000000", colored: true },
        "monokai":    { d0: "#1E1F1C", d1: "#272822", lt: "#F8F8F2", win: "#1E1F1C", elevated: "#23241F", card: "#2D2E27", cardHover: "#3E3D32", cardPressed: "#272822", flyout: "#272822", flyoutBorder: "#49483E", border: "#49483E", divider: "#3E3D32", text: "#F8F8F2", muted: "#CFCFC2", faint: "#8F908A", control: "#2D2E27", controlHover: "#3E3D32", controlPressed: "#272822", colored: true },
        "night-owl":  { d0: "#011627", d1: "#071D2F", lt: "#F0F4F8", win: "#011627", elevated: "#061A2B", card: "#0B253A", cardHover: "#12314A", cardPressed: "#071D2F", flyout: "#071D2F", flyoutBorder: "#1D3B53", border: "#1D3B53", divider: "#15324A", text: "#D6DEEB", muted: "#A9B8C8", faint: "#637777", control: "#0B253A", controlHover: "#12314A", controlPressed: "#071D2F", colored: true },
        "nord":       { d0: "#2E3440", d1: "#3B4252", lt: "#D8DEE9", win: "#2E3440", elevated: "#343B49", card: "#3B4252", cardHover: "#434C5E", cardPressed: "#343B49", flyout: "#3B4252", flyoutBorder: "#4C566A", border: "#4C566A", divider: "#434C5E", text: "#ECEFF4", muted: "#D8DEE9", faint: "#A7B1C2", control: "#3B4252", controlHover: "#434C5E", controlPressed: "#343B49", colored: true },
        "notion":     { d0: "#191919", d1: "#202020", lt: "#FFFFFF", win: "#191919", elevated: "#1F1F1F", card: "#262626", cardHover: "#303030", cardPressed: "#202020", flyout: "#202020", flyoutBorder: "#3A3A3A", border: "#3A3A3A", divider: "#303030", text: "#F7F7F5", muted: "#B7B7B5", faint: "#858585", control: "#262626", controlHover: "#303030", controlPressed: "#202020", colored: true },
        "one":        { d0: "#1E222A", d1: "#282C34", lt: "#FAFAFA", win: "#1E222A", elevated: "#242832", card: "#2F343F", cardHover: "#3A404D", cardPressed: "#282C34", flyout: "#282C34", flyoutBorder: "#444B5A", border: "#444B5A", divider: "#353B47", text: "#ABB2BF", muted: "#9DA5B4", faint: "#6B7280", control: "#2F343F", controlHover: "#3A404D", controlPressed: "#282C34", colored: true },
        "oscurange":  { d0: "#140D07", d1: "#21150B", lt: "#FFF4E6", win: "#140D07", elevated: "#1A1109", card: "#2B1B0E", cardHover: "#3E2815", cardPressed: "#21150B", flyout: "#21150B", flyoutBorder: "#5A3920", border: "#5A3920", divider: "#3E2815", text: "#FFEAD1", muted: "#F4B26A", faint: "#A87742", control: "#2B1B0E", controlHover: "#3E2815", controlPressed: "#21150B", colored: true },
        "raycast":    { d0: "#151515", d1: "#1F1F1F", lt: "#FFFFFF", win: "#151515", elevated: "#1A1A1A", card: "#292929", cardHover: "#363636", cardPressed: "#202020", flyout: "#1F1F1F", flyoutBorder: "#3A3A3A", border: "#3A3A3A", divider: "#303030", text: "#FFFFFF", muted: "#C9C9C9", faint: "#8E8E8E", control: "#292929", controlHover: "#363636", controlPressed: "#202020", colored: true },
        "rose-pine":  { d0: "#191724", d1: "#1F1D2E", lt: "#FAF4ED", win: "#191724", elevated: "#1F1D2E", card: "#26233A", cardHover: "#403D52", cardPressed: "#1F1D2E", flyout: "#1F1D2E", flyoutBorder: "#403D52", border: "#403D52", divider: "#312F44", text: "#E0DEF4", muted: "#C4A7E7", faint: "#908CAA", control: "#26233A", controlHover: "#403D52", controlPressed: "#1F1D2E", colored: true },
        "sentry":     { d0: "#17111F", d1: "#21162B", lt: "#FAF7FF", win: "#17111F", elevated: "#1D1427", card: "#2A1D38", cardHover: "#3B294F", cardPressed: "#21162B", flyout: "#21162B", flyoutBorder: "#493461", border: "#493461", divider: "#372748", text: "#F7F0FF", muted: "#D2B7F2", faint: "#9B82B7", control: "#2A1D38", controlHover: "#3B294F", controlPressed: "#21162B", colored: true },
        "solarized":  { d0: "#002B36", d1: "#073642", lt: "#FDF6E3", win: "#002B36", elevated: "#06313C", card: "#0A3A46", cardHover: "#164B58", cardPressed: "#073642", flyout: "#073642", flyoutBorder: "#586E75", border: "#586E75", divider: "#164B58", text: "#EEE8D5", muted: "#93A1A1", faint: "#839496", control: "#0A3A46", controlHover: "#164B58", controlPressed: "#073642", colored: true },
        "temple":     { d0: "#10150C", d1: "#192111", lt: "#F6FFE8", win: "#10150C", elevated: "#141B0F", card: "#202B15", cardHover: "#2F3E20", cardPressed: "#192111", flyout: "#192111", flyoutBorder: "#46552D", border: "#46552D", divider: "#344421", text: "#ECF7D6", muted: "#C6E478", faint: "#8AA253", control: "#202B15", controlHover: "#2F3E20", controlPressed: "#192111", colored: true },
        "tokyo-night": { d0: "#16161E", d1: "#1A1B26", lt: "#D5D6DB", win: "#16161E", elevated: "#181923", card: "#24283B", cardHover: "#292E42", cardPressed: "#1F2335", flyout: "#1A1B26", flyoutBorder: "#414868", border: "#414868", divider: "#292E42", text: "#C0CAF5", muted: "#A9B1D6", faint: "#565F89", control: "#24283B", controlHover: "#292E42", controlPressed: "#1F2335", colored: true },
        "vercel":     { d0: "#000000", d1: "#111111", lt: "#FFFFFF", win: "#000000", elevated: "#080808", card: "#171717", cardHover: "#262626", cardPressed: "#111111", flyout: "#111111", flyoutBorder: "#333333", border: "#333333", divider: "#262626", text: "#FAFAFA", muted: "#A3A3A3", faint: "#737373", control: "#171717", controlHover: "#262626", controlPressed: "#111111", colored: true },
        "vscode-plus": { d0: "#1E1E1E", d1: "#252526", lt: "#FFFFFF", win: "#1E1E1E", elevated: "#202020", card: "#2D2D30", cardHover: "#37373D", cardPressed: "#252526", flyout: "#252526", flyoutBorder: "#3E3E42", border: "#3E3E42", divider: "#333333", text: "#D4D4D4", muted: "#B5B5B5", faint: "#858585", control: "#2D2D30", controlHover: "#37373D", controlPressed: "#252526", colored: true },
        "xcode":      { d0: "#17191F", d1: "#20242D", lt: "#FFFFFF", win: "#17191F", elevated: "#1C2028", card: "#272D38", cardHover: "#323A49", cardPressed: "#20242D", flyout: "#20242D", flyoutBorder: "#3B4658", border: "#3B4658", divider: "#303746", text: "#E7EEF8", muted: "#B7C4D8", faint: "#7D8BA3", control: "#272D38", controlHover: "#323A49", controlPressed: "#20242D", colored: true }
    })
    readonly property var _lightPresetMap: ({
        "default":     { bg: "#F3F3F3", layer: "#FFFFFF", hover: "#F4F4F4", press: "#ECECEC", border: "#D6D6D6", divider: "#E2E2E2", text: "#1F1F1F", muted: "#666666", faint: "#8A8A8A" },
        "absolutely":  { bg: "#F5F3ED", layer: "#FFFFFF", hover: "#EFECE3", press: "#E4E0D5", border: "#D5CFC1", divider: "#E6E1D4", text: "#32302C", muted: "#686358", faint: "#8E897D" },
        "ayu":         { bg: "#F8F6F0", layer: "#FFFFFF", hover: "#F1EDE4", press: "#E8E0D2", border: "#D8D0C0", divider: "#E6DED0", text: "#2F302A", muted: "#68655D", faint: "#8C877C" },
        "catppuccin":  { bg: "#EFF1F5", layer: "#FFFFFF", hover: "#E9ECF2", press: "#DDE2EC", border: "#CAD0DC", divider: "#DCE0E8", text: "#4C4F69", muted: "#6C6F85", faint: "#8C8FA1" },
        "codex":       { bg: "#F5F6F8", layer: "#FFFFFF", hover: "#EAEEF5", press: "#DDE3ED", border: "#D6DCE7", divider: "#E4E8F0", text: "#1C2430", muted: "#5C6675", faint: "#7C8796" },
        "dracula":     { bg: "#F7F4FC", layer: "#FFFFFF", hover: "#F0EAF8", press: "#E6DEF0", border: "#D8CCE4", divider: "#E7DEEF", text: "#2B2334", muted: "#6C5D7E", faint: "#9385A2" },
        "everforest":  { bg: "#F7F5EC", layer: "#FFFFFF", hover: "#EFEBDD", press: "#E6DFCC", border: "#D5CAB4", divider: "#E5DECD", text: "#3C4033", muted: "#677151", faint: "#8B9278" },
        "github":      { bg: "#F6F8FA", layer: "#FFFFFF", hover: "#F0F3F6", press: "#EAEFF4", border: "#D0D7DE", divider: "#D8DEE4", text: "#24292F", muted: "#57606A", faint: "#6E7781" },
        "gruvbox":     { bg: "#F8F3E4", layer: "#FFFFFF", hover: "#EFE7D4", press: "#E4D9C2", border: "#D6C7AC", divider: "#E7DCC8", text: "#3C3836", muted: "#665C54", faint: "#928374" },
        "linear":      { bg: "#F7F7F8", layer: "#FFFFFF", hover: "#EEEFF4", press: "#E2E4EC", border: "#D8DAE4", divider: "#E6E8EF", text: "#20242D", muted: "#5F6675", faint: "#858C9A" },
        "lobster":     { bg: "#FAF3F5", layer: "#FFFFFF", hover: "#F3E7EB", press: "#EAD9DF", border: "#DBC4CC", divider: "#E9D8DE", text: "#3B1D29", muted: "#835466", faint: "#A77A89" },
        "material":    { bg: "#F1F4F6", layer: "#FFFFFF", hover: "#E9EEF2", press: "#DEE6EC", border: "#CAD4DC", divider: "#E0E8ED", text: "#263238", muted: "#546E7A", faint: "#78909C" },
        "matrix":      { bg: "#F1FAF1", layer: "#FFFFFF", hover: "#E7F2E7", press: "#DAE9DA", border: "#C4D8C4", divider: "#DCEADC", text: "#0F2A13", muted: "#2F6F3A", faint: "#5D9567" },
        "midnight":    { bg: "#F3F3F3", layer: "#FFFFFF", hover: "#F4F4F4", press: "#ECECEC", border: "#D6D6D6", divider: "#E2E2E2", text: "#1F1F1F", muted: "#666666", faint: "#8A8A8A" },
        "monokai":     { bg: "#F7F6EF", layer: "#FFFFFF", hover: "#EFEEE4", press: "#E5E2D4", border: "#D5CEB8", divider: "#E6E1CF", text: "#272822", muted: "#6F6B58", faint: "#918C76" },
        "night-owl":   { bg: "#F0F4F8", layer: "#FAFDFF", hover: "#E3EEF7", press: "#D7E5F0", border: "#C4D3DF", divider: "#DAE6EF", text: "#183348", muted: "#4D6477", faint: "#768A9A" },
        "nord":        { bg: "#ECEFF4", layer: "#F7F9FC", hover: "#E5E9F0", press: "#D8DEE9", border: "#C7CFDC", divider: "#D8DEE9", text: "#2E3440", muted: "#4C566A", faint: "#6B7587" },
        "notion":      { bg: "#FFFFFF", layer: "#FBFBFA", hover: "#F1F1EF", press: "#E9E9E7", border: "#D9D9D6", divider: "#E7E7E4", text: "#37352F", muted: "#6B6A66", faint: "#9B9A97" },
        "one":         { bg: "#FAFAFA", layer: "#FFFFFF", hover: "#EDEFF3", press: "#E0E4EA", border: "#D2D7E0", divider: "#E2E5EA", text: "#383A42", muted: "#6B7080", faint: "#8D93A2" },
        "oscurange":   { bg: "#FAF4EC", layer: "#FFFFFF", hover: "#F1E7DA", press: "#E7DAC9", border: "#D8C6AE", divider: "#E8DBC9", text: "#3A2515", muted: "#775437", faint: "#9D7959" },
        "raycast":     { bg: "#FFFFFF", layer: "#FAFAFA", hover: "#F0F0F0", press: "#E7E7E7", border: "#D8D8D8", divider: "#E5E5E5", text: "#1F1F1F", muted: "#666666", faint: "#909090" },
        "rose-pine":   { bg: "#FAF6F1", layer: "#FFFFFF", hover: "#F1EBE6", press: "#E8DED8", border: "#D8CAC4", divider: "#E8DED8", text: "#575279", muted: "#797593", faint: "#9893A5" },
        "sentry":      { bg: "#FAF7FF", layer: "#FFFFFF", hover: "#EFE7FA", press: "#E4D6F5", border: "#D4BEEB", divider: "#E8DCF5", text: "#2F253B", muted: "#6E5B84", faint: "#9582A8" },
        "solarized":   { bg: "#FAF5E6", layer: "#FFFFFF", hover: "#F0E9D6", press: "#E6DCC6", border: "#D7C9AE", divider: "#E8DEC8", text: "#586E75", muted: "#657B83", faint: "#93A1A1" },
        "temple":      { bg: "#F6FAEF", layer: "#FFFFFF", hover: "#EDF3E1", press: "#E3EBD4", border: "#CDD9B7", divider: "#E1EBD3", text: "#26331B", muted: "#657A3C", faint: "#8B9D68" },
        "tokyo-night": { bg: "#D5D6DB", layer: "#F5F6FA", hover: "#E7E9F2", press: "#DDE0EA", border: "#C8CEDD", divider: "#DDE0EA", text: "#343B58", muted: "#565F89", faint: "#7B829E" },
        "vercel":      { bg: "#FFFFFF", layer: "#FAFAFA", hover: "#F0F0F0", press: "#E7E7E7", border: "#D8D8D8", divider: "#E5E5E5", text: "#111111", muted: "#666666", faint: "#8C8C8C" },
        "vscode-plus": { bg: "#FFFFFF", layer: "#FAFAFA", hover: "#F0F3F7", press: "#E5EBF2", border: "#D0D7E2", divider: "#E1E6ED", text: "#1E1E1E", muted: "#5F6B7A", faint: "#7F8996" },
        "xcode":       { bg: "#FFFFFF", layer: "#F8FAFD", hover: "#EAF2FC", press: "#DCEAF8", border: "#C8D6E6", divider: "#DDE7F2", text: "#1B2430", muted: "#56677F", faint: "#7C8DA3" }
    })
    readonly property var _pal: _presetMap[preset] !== undefined ? _presetMap[preset] : _presetMap["default"]
    readonly property var _lightPal: _lightPresetMap[preset] !== undefined ? _lightPresetMap[preset] : _lightPresetMap["default"]
    readonly property bool presetColored: _pal.colored === true || preset === "nord" || preset === "solarized" || preset === "dracula" || preset === "catppuccin" || (preset === "midnight" && dark)

    function _accentIsLight(c) {
        return (c.r * 0.299 + c.g * 0.587 + c.b * 0.114) > 0.62
    }

    // ---- Accent derivatives --------------------------------------------
    readonly property color accentHover: dark ? Qt.lighter(accent, 1.12) : Qt.darker(accent, 1.06)
    readonly property color accentPressed: dark ? Qt.lighter(accent, 1.22) : Qt.darker(accent, 1.14)
    readonly property color accentText: _accentIsLight(accent) ? "#111111" : "#FFFFFF"
    readonly property color accentSoft: dark ? Qt.rgba(accent.r, accent.g, accent.b, 0.22)
                                             : Qt.rgba(accent.r, accent.g, accent.b, 0.20)
    // Curated accent palette for the Appearance Studio swatch grid.
    readonly property var accentSwatches: ["#0078D4", "#0099BC", "#00B294", "#107C10",
        "#744DA9", "#BD93F9", "#CBA6F7", "#B146C2", "#E3008C", "#C30052", "#E81123", "#F7630C", "#FFB900", "#498205"]

    // ---- Surfaces (Fluent Mica / layer palette) ------------------------
    readonly property color bg: dark ? _pal.d0 : _lightPal.bg

    function _palColor(name, fallback) {
        var value = _pal[name]
        return value === undefined || value === "" ? fallback : value
    }

    function _hexWithAlpha(hex, alpha) {
        var h = ("" + hex).replace("#", "")
        if (h.length < 6)
            return hex
        var r = parseInt(h.substr(0, 2), 16)
        var g = parseInt(h.substr(2, 2), 16)
        var b = parseInt(h.substr(4, 2), 16)
        return Qt.rgba(r / 255, g / 255, b / 255, alpha)
    }

    function _palLayer(name, fallback, alpha) {
        if (!(dark && presetColored))
            return _palColor(name, fallback)
        var value = _pal[name]
        if (value === undefined || value === "")
            return fallback
        return isTranslucent ? _hexWithAlpha(value, alpha) : value
    }

    function _lightColor(name, fallback) {
        var value = _lightPal[name]
        return value === undefined || value === "" ? fallback : value
    }

    function _lightLayer(name, fallback, alpha) {
        if (!presetColored)
            return fallback
        var value = _lightPal[name]
        if (value === undefined || value === "")
            return fallback
        return isTranslucent ? _hexWithAlpha(value, alpha) : value
    }

    function _lightBackground(alpha) {
        return _hexWithAlpha(_lightPal.bg, alpha)
    }

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
    readonly property real darkPaletteWindowAlpha: 0.50
    readonly property real darkPalettePanelAlpha: 0.46
    readonly property real lightGlassWindowAlpha: 0.22
    readonly property real lightGlassPanelAlpha: 0.18
    readonly property real lightPaletteWindowAlpha: 0.30
    readonly property real lightPalettePanelAlpha: 0.32
    readonly property real lightGlassCardAlpha: 0.50
    readonly property real lightGlassCardHoverAlpha: 0.62
    readonly property real lightGlassCardPressedAlpha: 0.70
    readonly property real lightGlassControlAlpha: 0.44
    readonly property real lightGlassControlHoverAlpha: 0.58
    readonly property real lightGlassControlPressedAlpha: 0.68

    readonly property color windowBase: baseTint !== ""
        ? (dark ? _darkTint(baseTint) : _lightTint(baseTint))
        : (presetColored
            ? (dark ? (_pal.win !== "" ? _pal.win : _pal.d0) : _pal.lt)
            : (backdropAvailable ? "transparent" : (dark ? _pal.d0 : _lightPal.bg)))
    readonly property color micaBase: "transparent"
    readonly property color contentPanel: dark
        ? _palLayer("card", Qt.rgba(1, 1, 1, 0.048), 0.44)
        : _lightLayer("bg", Qt.rgba(1, 1, 1, 0.18), lightPalettePanelAlpha)
    readonly property color railPanel: dark ? _palLayer("control", Qt.rgba(1, 1, 1, 0.065), 0.72) : _lightLayer("press", Qt.rgba(1, 1, 1, 0.34), 0.70)
    readonly property color chromePanel: dark
        ? _palLayer("control", Qt.rgba(1, 1, 1, 0.030), 0.26)
        : Qt.rgba(1, 1, 1, 0.22)
    readonly property color bgElevated: dark ? Qt.rgba(1, 1, 1, 0.016) : Qt.rgba(1, 1, 1, 0.55)
    readonly property color card: dark ? Qt.rgba(1, 1, 1, 0.040) : Qt.rgba(1, 1, 1, 0.70)
    readonly property color cardHover: dark ? Qt.rgba(1, 1, 1, 0.0837) : Qt.rgba(0, 0, 0, 0.024)
    readonly property color cardPressed: dark ? Qt.rgba(1, 1, 1, 0.0326) : Qt.rgba(0, 0, 0, 0.040)
    readonly property color navHover: dark ? cardHover : Qt.rgba(accent.r, accent.g, accent.b, 0.11)
    readonly property color flyout: dark ? _pal.d1 : "#FBFBFB"
    readonly property color flyoutBorder: dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0, 0, 0, 0.13)

    // ---- Lines ----------------------------------------------------------
    readonly property color borderSolid: dark ? Qt.rgba(1, 1, 1, 0.09) : Qt.rgba(0, 0, 0, 0.07)
    readonly property color divider: dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(0, 0, 0, 0.06)
    readonly property color borderBottom: dark ? Qt.rgba(0, 0, 0, 0.20) : Qt.rgba(0, 0, 0, 0.10)

    // ---- Text (Fluent TextFillColor ramp) ------------------------------
    readonly property color text: dark ? _palColor("text", Qt.rgba(1, 1, 1, 1.0)) : _lightColor("text", Qt.rgba(0, 0, 0, 0.89))
    readonly property color textMuted: dark ? _palColor("muted", Qt.rgba(1, 1, 1, 0.79)) : _lightColor("muted", Qt.rgba(0, 0, 0, 0.61))
    readonly property color textFaint: dark ? _palColor("faint", Qt.rgba(1, 1, 1, 0.55)) : _lightColor("faint", Qt.rgba(0, 0, 0, 0.45))

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
