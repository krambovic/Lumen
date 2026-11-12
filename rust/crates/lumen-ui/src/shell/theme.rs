//! Theme engine ported from `windows/xray_fluent/qml_app/qml/Theme.qml`.
//!
//! Central design tokens (Windows 11 Fluent look). `dark`/`accent` plus the
//! 2.0 appearance inputs (density / cornerRadius / fontScale / fontFamilyName
//! / animations / backdrop / preset) are driven from the shell out of saved
//! settings, so every consumer reads them through these tokens.

use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, Style, Vec2};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Hex parse used across Theme.qml (`#RRGGBB` / `#RRGGBBAA`); unknown or
/// shorter values keep the caller's fallback, exactly like Qt's color parser.
pub fn hex_color(hex: &str) -> Option<Color32> {
    let hex = hex.trim();
    let hex = hex.strip_prefix('#')?;
    if hex.len() < 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    let a = if hex.len() >= 8 {
        u8::from_str_radix(&hex[6..8], 16).ok()?
    } else {
        255
    };
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// Qt.rgba(r, g, b, a) with float channels.
pub fn qt_rgba(r: f64, g: f64, b: f64, a: f64) -> Color32 {
    let clamp = |v: f64| -> u8 { (v * 255.0).round().clamp(0.0, 255.0) as u8 };
    Color32::from_rgba_unmultiplied(clamp(r), clamp(g), clamp(b), clamp(a))
}

fn alpha_u8(alpha: f64) -> u8 {
    (alpha * 255.0).round().clamp(0.0, 255.0) as u8
}

/// `Qt.rgba(color.r, color.g, color.b, alpha)` on an existing Color32.
fn with_alpha(color: Color32, alpha: f64) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha_u8(alpha))
}

fn blend(fore: Color32, back: Color32, a: f64) -> Color32 {
    let ch = |c: Color32| -> (f64, f64, f64) {
        (
            c.r() as f64 / 255.0,
            c.g() as f64 / 255.0,
            c.b() as f64 / 255.0,
        )
    };
    let (fr, fg, fb) = ch(fore);
    let (br, bg, bb) = ch(back);
    qt_rgba(
        fr * a + br * (1.0 - a),
        fg * a + bg * (1.0 - a),
        fb * a + bb * (1.0 - a),
        1.0,
    )
}

// ── QColor HSV helpers (Qt.lighter / Qt.darker semantics) ────────────────
//
// QML `Qt.lighter(c, f)` == `QColor::lighter(int(f * 100))`:
//   sat = sat - sat*(factor-100)/100; val = val*factor/100 (clamped).
// QML `Qt.darker(c, f)` == `QColor::darker(int(f * 100))`: val = val*100/factor.

/// Replace RGB channels while keeping the original alpha.
fn repaint_rgb(color: Color32, r: f64, g: f64, b: f64) -> Color32 {
    let clamp = |v: f64| -> u8 { (v * 255.0).round().clamp(0.0, 255.0) as u8 };
    Color32::from_rgba_unmultiplied(clamp(r), clamp(g), clamp(b), color.a())
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    // returns hue 0..360, sat 0..1, val 0..1
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let hue = if d == 0.0 {
        0.0
    } else if max == r {
        let h = ((g - b) / d) % 6.0;
        let h = if h < 0.0 { h + 6.0 } else { h };
        h * 60.0
    } else if max == g {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    };
    let sat = if max == 0.0 { 0.0 } else { d / max };
    (hue, sat, max)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let c = v * s;
    let h = h.rem_euclid(360.0);
    let hp = h / 60.0;
    let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (r1 + m, g1 + m, b1 + m)
}

/// QColor::lighter with an integer factor percentage (112 => Qt.lighter(c, 1.12)).
pub fn qcolor_lighter(color: Color32, factor: i32) -> Color32 {
    let factor = if factor <= 0 { 100 } else { factor };
    if factor == 100 {
        return color;
    }
    if factor < 100 {
        return qcolor_darker(color, (10000 / factor).max(1));
    }
    let (h, s, v) = rgb_to_hsv(
        color.r() as f64 / 255.0,
        color.g() as f64 / 255.0,
        color.b() as f64 / 255.0,
    );
    let f = factor as f64 / 100.0;
    let v = v * f;
    let s = (s - (v - 1.0).max(0.0)).clamp(0.0, 1.0);
    let v = v.min(1.0);
    let (r, g, b) = hsv_to_rgb(h, s, v);
    repaint_rgb(color, r, g, b)
}

/// QColor::darker with an integer factor percentage (106 => Qt.darker(c, 1.06)).
pub fn qcolor_darker(color: Color32, factor: i32) -> Color32 {
    let factor = if factor <= 0 { 100 } else { factor };
    if factor < 100 {
        return qcolor_lighter(color, (10000 / factor).max(1));
    }
    if factor == 100 {
        return color;
    }
    let (h, s, v) = rgb_to_hsv(
        color.r() as f64 / 255.0,
        color.g() as f64 / 255.0,
        color.b() as f64 / 255.0,
    );
    let v = (v * 100.0 / factor as f64).clamp(0.0, 1.0);
    let (r, g, b) = hsv_to_rgb(h, s, v);
    repaint_rgb(color, r, g, b)
}

fn qml_lighter(color: Color32, factor: f64) -> Color32 {
    qcolor_lighter(color, (factor * 100.0).round() as i32)
}

fn qml_darker(color: Color32, factor: f64) -> Color32 {
    qcolor_darker(color, (factor * 100.0).round() as i32)
}

// ── Theme presets ─────────────────────────────────────────────────────────

/// A dark-side preset palette (Theme.qml `_presetMap` entries). `None` means
/// the QML object did not declare the key ("" behaves the same).
#[derive(Clone, Copy)]
pub struct PresetPalette {
    pub d0: &'static str,
    pub d1: &'static str,
    pub lt: &'static str,
    pub win: Option<&'static str>,
    pub elevated: Option<&'static str>,
    pub card: Option<&'static str>,
    pub card_hover: Option<&'static str>,
    pub card_pressed: Option<&'static str>,
    pub flyout: Option<&'static str>,
    pub flyout_border: Option<&'static str>,
    pub border: Option<&'static str>,
    pub divider: Option<&'static str>,
    pub text: Option<&'static str>,
    pub muted: Option<&'static str>,
    pub faint: Option<&'static str>,
    pub control: Option<&'static str>,
    pub control_hover: Option<&'static str>,
    pub control_pressed: Option<&'static str>,
    pub colored: bool,
}

const fn dark_preset(
    d0: &'static str,
    d1: &'static str,
    lt: &'static str,
    win: Option<&'static str>,
    colored: bool,
) -> PresetPalette {
    PresetPalette {
        d0,
        d1,
        lt,
        win,
        elevated: None,
        card: None,
        card_hover: None,
        card_pressed: None,
        flyout: None,
        flyout_border: None,
        border: None,
        divider: None,
        text: None,
        muted: None,
        faint: None,
        control: None,
        control_hover: None,
        control_pressed: None,
        colored,
    }
}

/// Dark surface palettes, keyed exactly like `_presetMap`.
pub fn dark_preset_map() -> BTreeMap<&'static str, PresetPalette> {
    let mut m: BTreeMap<&'static str, PresetPalette> = BTreeMap::new();
    m.insert(
        "default",
        dark_preset("#202020", "#2C2C2C", "#F3F3F3", None, false),
    );
    m.insert(
        "absolutely",
        PresetPalette {
            elevated: Some("#21211F"),
            card: Some("#383835"),
            card_hover: Some("#42423E"),
            card_pressed: Some("#31312E"),
            flyout: Some("#252523"),
            flyout_border: Some("#474744"),
            border: Some("#42423E"),
            divider: Some("#383835"),
            text: Some("#F9F9F7"),
            muted: Some("#B5B5B0"),
            faint: Some("#888882"),
            control: Some("#383835"),
            control_hover: Some("#42423E"),
            control_pressed: Some("#31312E"),
            ..dark_preset("#2D2D2B", "#21211F", "#F5F3ED", Some("#2D2D2B"), true)
        },
    );
    m.insert(
        "ayu",
        PresetPalette {
            elevated: Some("#101521"),
            card: Some("#151B26"),
            card_hover: Some("#1F2937"),
            card_pressed: Some("#10151E"),
            flyout: Some("#11151C"),
            flyout_border: Some("#2D3646"),
            border: Some("#2D3646"),
            divider: Some("#232B38"),
            text: Some("#E6E1CF"),
            muted: Some("#B3B1AD"),
            faint: Some("#7F8692"),
            control: Some("#151B26"),
            control_hover: Some("#1F2937"),
            control_pressed: Some("#10151E"),
            ..dark_preset("#0B0E14", "#11151C", "#FAFAFA", Some("#0B0E14"), true)
        },
    );
    m.insert(
        "dracula",
        PresetPalette {
            elevated: Some("#2D3040"),
            card: Some("#343746"),
            card_hover: Some("#44475A"),
            card_pressed: Some("#303342"),
            flyout: Some("#282A36"),
            flyout_border: Some("#44475A"),
            border: Some("#44475A"),
            divider: Some("#3B3E50"),
            text: Some("#F8F8F2"),
            muted: Some("#D7D2E8"),
            faint: Some("#A8A6B3"),
            control: Some("#343746"),
            control_hover: Some("#44475A"),
            control_pressed: Some("#303342"),
            ..dark_preset("#282A36", "#343746", "#F8F8F2", Some("#282A36"), true)
        },
    );
    m.insert(
        "catppuccin",
        PresetPalette {
            elevated: Some("#181825"),
            card: Some("#1E1E2E"),
            card_hover: Some("#313244"),
            card_pressed: Some("#181825"),
            flyout: Some("#1E1E2E"),
            flyout_border: Some("#313244"),
            border: Some("#313244"),
            divider: Some("#313244"),
            text: Some("#CDD6F4"),
            muted: Some("#A6ADC8"),
            faint: Some("#7F849C"),
            control: Some("#1E1E2E"),
            control_hover: Some("#313244"),
            control_pressed: Some("#181825"),
            ..dark_preset("#11111B", "#1E1E2E", "#EFF1F5", Some("#11111B"), true)
        },
    );
    m.insert(
        "codex",
        PresetPalette {
            elevated: Some("#141720"),
            card: Some("#1D212B"),
            card_hover: Some("#2B3040"),
            card_pressed: Some("#171A22"),
            flyout: Some("#181B23"),
            flyout_border: Some("#2B3040"),
            border: Some("#2B3040"),
            divider: Some("#252A36"),
            text: Some("#F4F6FB"),
            muted: Some("#AEB6C7"),
            faint: Some("#778092"),
            control: Some("#1D212B"),
            control_hover: Some("#2B3040"),
            control_pressed: Some("#171A22"),
            ..dark_preset("#0F1117", "#181B23", "#F5F5F5", Some("#0F1117"), true)
        },
    );
    m.insert(
        "everforest",
        PresetPalette {
            elevated: Some("#232A2E"),
            card: Some("#2E383C"),
            card_hover: Some("#3A464A"),
            card_pressed: Some("#252D31"),
            flyout: Some("#272E33"),
            flyout_border: Some("#414B50"),
            border: Some("#414B50"),
            divider: Some("#374145"),
            text: Some("#D3C6AA"),
            muted: Some("#A7C080"),
            faint: Some("#859289"),
            control: Some("#2E383C"),
            control_hover: Some("#3A464A"),
            control_pressed: Some("#252D31"),
            ..dark_preset("#1E2326", "#272E33", "#FDF6E3", Some("#1E2326"), true)
        },
    );
    m.insert(
        "github",
        PresetPalette {
            elevated: Some("#10161F"),
            card: Some("#161B22"),
            card_hover: Some("#21262D"),
            card_pressed: Some("#151A21"),
            flyout: Some("#161B22"),
            flyout_border: Some("#30363D"),
            border: Some("#30363D"),
            divider: Some("#21262D"),
            text: Some("#E6EDF3"),
            muted: Some("#8B949E"),
            faint: Some("#6E7681"),
            control: Some("#21262D"),
            control_hover: Some("#30363D"),
            control_pressed: Some("#161B22"),
            ..dark_preset("#0D1117", "#161B22", "#F6F8FA", Some("#0D1117"), true)
        },
    );
    m.insert(
        "gruvbox",
        PresetPalette {
            elevated: Some("#242422"),
            card: Some("#32302F"),
            card_hover: Some("#3C3836"),
            card_pressed: Some("#282828"),
            flyout: Some("#282828"),
            flyout_border: Some("#504945"),
            border: Some("#504945"),
            divider: Some("#3C3836"),
            text: Some("#EBDBB2"),
            muted: Some("#D5C4A1"),
            faint: Some("#928374"),
            control: Some("#32302F"),
            control_hover: Some("#3C3836"),
            control_pressed: Some("#282828"),
            ..dark_preset("#1D2021", "#282828", "#FBF1C7", Some("#1D2021"), true)
        },
    );
    m.insert(
        "linear",
        PresetPalette {
            elevated: Some("#0D0F16"),
            card: Some("#181B25"),
            card_hover: Some("#222635"),
            card_pressed: Some("#11131A"),
            flyout: Some("#11131A"),
            flyout_border: Some("#303443"),
            border: Some("#303443"),
            divider: Some("#252938"),
            text: Some("#F7F8FA"),
            muted: Some("#A8AFBF"),
            faint: Some("#737B8C"),
            control: Some("#181B25"),
            control_hover: Some("#222635"),
            control_pressed: Some("#11131A"),
            ..dark_preset("#08090D", "#11131A", "#F7F7F8", Some("#08090D"), true)
        },
    );
    m.insert(
        "lobster",
        PresetPalette {
            elevated: Some("#231018"),
            card: Some("#351A25"),
            card_hover: Some("#492434"),
            card_pressed: Some("#2A151D"),
            flyout: Some("#2A151D"),
            flyout_border: Some("#5A2D3E"),
            border: Some("#5A2D3E"),
            divider: Some("#472333"),
            text: Some("#FFE8EF"),
            muted: Some("#F2B8C6"),
            faint: Some("#B98392"),
            control: Some("#351A25"),
            control_hover: Some("#492434"),
            control_pressed: Some("#2A151D"),
            ..dark_preset("#1B0D13", "#2A151D", "#FFF1F4", Some("#1B0D13"), true)
        },
    );
    m.insert(
        "material",
        PresetPalette {
            elevated: Some("#151823"),
            card: Some("#202331"),
            card_hover: Some("#2B3040"),
            card_pressed: Some("#191C27"),
            flyout: Some("#1A1C25"),
            flyout_border: Some("#33394A"),
            border: Some("#33394A"),
            divider: Some("#293042"),
            text: Some("#EEFFFF"),
            muted: Some("#B0BEC5"),
            faint: Some("#697884"),
            control: Some("#202331"),
            control_hover: Some("#2B3040"),
            control_pressed: Some("#191C27"),
            ..dark_preset("#0F111A", "#1A1C25", "#ECEFF1", Some("#0F111A"), true)
        },
    );
    m.insert(
        "matrix",
        PresetPalette {
            elevated: Some("#020A04"),
            card: Some("#06160A"),
            card_hover: Some("#0B2A10"),
            card_pressed: Some("#031006"),
            flyout: Some("#031006"),
            flyout_border: Some("#145C22"),
            border: Some("#145C22"),
            divider: Some("#0D3515"),
            text: Some("#D9FFE2"),
            muted: Some("#7CFF91"),
            faint: Some("#3FAF52"),
            control: Some("#06160A"),
            control_hover: Some("#0B2A10"),
            control_pressed: Some("#031006"),
            ..dark_preset("#000000", "#031006", "#E8FFE8", Some("#000000"), true)
        },
    );
    m.insert(
        "midnight",
        dark_preset("#000000", "#1A1A1A", "#F3F3F3", Some("#000000"), true),
    );
    m.insert(
        "monokai",
        PresetPalette {
            elevated: Some("#23241F"),
            card: Some("#2D2E27"),
            card_hover: Some("#3E3D32"),
            card_pressed: Some("#272822"),
            flyout: Some("#272822"),
            flyout_border: Some("#49483E"),
            border: Some("#49483E"),
            divider: Some("#3E3D32"),
            text: Some("#F8F8F2"),
            muted: Some("#CFCFC2"),
            faint: Some("#8F908A"),
            control: Some("#2D2E27"),
            control_hover: Some("#3E3D32"),
            control_pressed: Some("#272822"),
            ..dark_preset("#1E1F1C", "#272822", "#F8F8F2", Some("#1E1F1C"), true)
        },
    );
    m.insert(
        "night-owl",
        PresetPalette {
            elevated: Some("#061A2B"),
            card: Some("#0B253A"),
            card_hover: Some("#12314A"),
            card_pressed: Some("#071D2F"),
            flyout: Some("#071D2F"),
            flyout_border: Some("#1D3B53"),
            border: Some("#1D3B53"),
            divider: Some("#15324A"),
            text: Some("#D6DEEB"),
            muted: Some("#A9B8C8"),
            faint: Some("#637777"),
            control: Some("#0B253A"),
            control_hover: Some("#12314A"),
            control_pressed: Some("#071D2F"),
            ..dark_preset("#011627", "#071D2F", "#F0F4F8", Some("#011627"), true)
        },
    );
    m.insert(
        "nord",
        PresetPalette {
            elevated: Some("#343B49"),
            card: Some("#3B4252"),
            card_hover: Some("#434C5E"),
            card_pressed: Some("#343B49"),
            flyout: Some("#3B4252"),
            flyout_border: Some("#4C566A"),
            border: Some("#4C566A"),
            divider: Some("#434C5E"),
            text: Some("#ECEFF4"),
            muted: Some("#D8DEE9"),
            faint: Some("#A7B1C2"),
            control: Some("#3B4252"),
            control_hover: Some("#434C5E"),
            control_pressed: Some("#343B49"),
            ..dark_preset("#2E3440", "#3B4252", "#D8DEE9", Some("#2E3440"), true)
        },
    );
    m.insert(
        "notion",
        PresetPalette {
            elevated: Some("#1F1F1F"),
            card: Some("#262626"),
            card_hover: Some("#303030"),
            card_pressed: Some("#202020"),
            flyout: Some("#202020"),
            flyout_border: Some("#3A3A3A"),
            border: Some("#3A3A3A"),
            divider: Some("#303030"),
            text: Some("#F7F7F5"),
            muted: Some("#B7B7B5"),
            faint: Some("#858585"),
            control: Some("#262626"),
            control_hover: Some("#303030"),
            control_pressed: Some("#202020"),
            ..dark_preset("#191919", "#202020", "#FFFFFF", Some("#191919"), true)
        },
    );
    m.insert(
        "one",
        PresetPalette {
            elevated: Some("#242832"),
            card: Some("#2F343F"),
            card_hover: Some("#3A404D"),
            card_pressed: Some("#282C34"),
            flyout: Some("#282C34"),
            flyout_border: Some("#444B5A"),
            border: Some("#444B5A"),
            divider: Some("#353B47"),
            text: Some("#ABB2BF"),
            muted: Some("#9DA5B4"),
            faint: Some("#6B7280"),
            control: Some("#2F343F"),
            control_hover: Some("#3A404D"),
            control_pressed: Some("#282C34"),
            ..dark_preset("#1E222A", "#282C34", "#FAFAFA", Some("#1E222A"), true)
        },
    );
    m.insert(
        "oscurange",
        PresetPalette {
            elevated: Some("#1A1109"),
            card: Some("#2B1B0E"),
            card_hover: Some("#3E2815"),
            card_pressed: Some("#21150B"),
            flyout: Some("#21150B"),
            flyout_border: Some("#5A3920"),
            border: Some("#5A3920"),
            divider: Some("#3E2815"),
            text: Some("#FFEAD1"),
            muted: Some("#F4B26A"),
            faint: Some("#A87742"),
            control: Some("#2B1B0E"),
            control_hover: Some("#3E2815"),
            control_pressed: Some("#21150B"),
            ..dark_preset("#140D07", "#21150B", "#FFF4E6", Some("#140D07"), true)
        },
    );
    m.insert(
        "raycast",
        PresetPalette {
            elevated: Some("#1A1A1A"),
            card: Some("#292929"),
            card_hover: Some("#363636"),
            card_pressed: Some("#202020"),
            flyout: Some("#1F1F1F"),
            flyout_border: Some("#3A3A3A"),
            border: Some("#3A3A3A"),
            divider: Some("#303030"),
            text: Some("#FFFFFF"),
            muted: Some("#C9C9C9"),
            faint: Some("#8E8E8E"),
            control: Some("#292929"),
            control_hover: Some("#363636"),
            control_pressed: Some("#202020"),
            ..dark_preset("#151515", "#1F1F1F", "#FFFFFF", Some("#151515"), true)
        },
    );
    m.insert(
        "rose-pine",
        PresetPalette {
            elevated: Some("#1F1D2E"),
            card: Some("#26233A"),
            card_hover: Some("#403D52"),
            card_pressed: Some("#1F1D2E"),
            flyout: Some("#1F1D2E"),
            flyout_border: Some("#403D52"),
            border: Some("#403D52"),
            divider: Some("#312F44"),
            text: Some("#E0DEF4"),
            muted: Some("#C4A7E7"),
            faint: Some("#908CAA"),
            control: Some("#26233A"),
            control_hover: Some("#403D52"),
            control_pressed: Some("#1F1D2E"),
            ..dark_preset("#191724", "#1F1D2E", "#FAF4ED", Some("#191724"), true)
        },
    );
    m.insert(
        "sentry",
        PresetPalette {
            elevated: Some("#1D1427"),
            card: Some("#2A1D38"),
            card_hover: Some("#3B294F"),
            card_pressed: Some("#21162B"),
            flyout: Some("#21162B"),
            flyout_border: Some("#493461"),
            border: Some("#493461"),
            divider: Some("#372748"),
            text: Some("#F7F0FF"),
            muted: Some("#D2B7F2"),
            faint: Some("#9B82B7"),
            control: Some("#2A1D38"),
            control_hover: Some("#3B294F"),
            control_pressed: Some("#21162B"),
            ..dark_preset("#17111F", "#21162B", "#FAF7FF", Some("#17111F"), true)
        },
    );
    m.insert(
        "solarized",
        PresetPalette {
            elevated: Some("#06313C"),
            card: Some("#0A3A46"),
            card_hover: Some("#164B58"),
            card_pressed: Some("#073642"),
            flyout: Some("#073642"),
            flyout_border: Some("#586E75"),
            border: Some("#586E75"),
            divider: Some("#164B58"),
            text: Some("#EEE8D5"),
            muted: Some("#93A1A1"),
            faint: Some("#839496"),
            control: Some("#0A3A46"),
            control_hover: Some("#164B58"),
            control_pressed: Some("#073642"),
            ..dark_preset("#002B36", "#073642", "#FDF6E3", Some("#002B36"), true)
        },
    );
    m.insert(
        "temple",
        PresetPalette {
            elevated: Some("#141B0F"),
            card: Some("#202B15"),
            card_hover: Some("#2F3E20"),
            card_pressed: Some("#192111"),
            flyout: Some("#192111"),
            flyout_border: Some("#46552D"),
            border: Some("#46552D"),
            divider: Some("#344421"),
            text: Some("#ECF7D6"),
            muted: Some("#C6E478"),
            faint: Some("#8AA253"),
            control: Some("#202B15"),
            control_hover: Some("#2F3E20"),
            control_pressed: Some("#192111"),
            ..dark_preset("#10150C", "#192111", "#F6FFE8", Some("#10150C"), true)
        },
    );
    m.insert(
        "tokyo-night",
        PresetPalette {
            elevated: Some("#181923"),
            card: Some("#24283B"),
            card_hover: Some("#292E42"),
            card_pressed: Some("#1F2335"),
            flyout: Some("#1A1B26"),
            flyout_border: Some("#414868"),
            border: Some("#414868"),
            divider: Some("#292E42"),
            text: Some("#C0CAF5"),
            muted: Some("#A9B1D6"),
            faint: Some("#565F89"),
            control: Some("#24283B"),
            control_hover: Some("#292E42"),
            control_pressed: Some("#1F2335"),
            ..dark_preset("#16161E", "#1A1B26", "#D5D6DB", Some("#16161E"), true)
        },
    );
    m.insert(
        "vercel",
        PresetPalette {
            elevated: Some("#080808"),
            card: Some("#171717"),
            card_hover: Some("#262626"),
            card_pressed: Some("#111111"),
            flyout: Some("#111111"),
            flyout_border: Some("#333333"),
            border: Some("#333333"),
            divider: Some("#262626"),
            text: Some("#FAFAFA"),
            muted: Some("#A3A3A3"),
            faint: Some("#737373"),
            control: Some("#171717"),
            control_hover: Some("#262626"),
            control_pressed: Some("#111111"),
            ..dark_preset("#000000", "#111111", "#FFFFFF", Some("#000000"), true)
        },
    );
    m.insert(
        "vscode-plus",
        PresetPalette {
            elevated: Some("#202020"),
            card: Some("#2D2D30"),
            card_hover: Some("#37373D"),
            card_pressed: Some("#252526"),
            flyout: Some("#252526"),
            flyout_border: Some("#3E3E42"),
            border: Some("#3E3E42"),
            divider: Some("#333333"),
            text: Some("#D4D4D4"),
            muted: Some("#B5B5B5"),
            faint: Some("#858585"),
            control: Some("#2D2D30"),
            control_hover: Some("#37373D"),
            control_pressed: Some("#252526"),
            ..dark_preset("#1E1E1E", "#252526", "#FFFFFF", Some("#1E1E1E"), true)
        },
    );
    m.insert(
        "xcode",
        PresetPalette {
            elevated: Some("#1C2028"),
            card: Some("#272D38"),
            card_hover: Some("#323A49"),
            card_pressed: Some("#20242D"),
            flyout: Some("#20242D"),
            flyout_border: Some("#3B4658"),
            border: Some("#3B4658"),
            divider: Some("#303746"),
            text: Some("#E7EEF8"),
            muted: Some("#B7C4D8"),
            faint: Some("#7D8BA3"),
            control: Some("#272D38"),
            control_hover: Some("#323A49"),
            control_pressed: Some("#20242D"),
            ..dark_preset("#17191F", "#20242D", "#FFFFFF", Some("#17191F"), true)
        },
    );
    m
}

/// Light-side palette (`_lightPresetMap` entries).
#[derive(Clone, Copy)]
pub struct LightPalette {
    pub bg: &'static str,
    pub layer: &'static str,
    pub hover: &'static str,
    pub press: &'static str,
    pub border: &'static str,
    pub divider: &'static str,
    pub text: &'static str,
    pub muted: &'static str,
    pub faint: &'static str,
}

const fn light_preset(
    bg: &'static str,
    layer: &'static str,
    hover: &'static str,
    press: &'static str,
    border: &'static str,
    divider: &'static str,
    text: &'static str,
    muted: &'static str,
    faint: &'static str,
) -> LightPalette {
    LightPalette {
        bg,
        layer,
        hover,
        press,
        border,
        divider,
        text,
        muted,
        faint,
    }
}

/// Light surface palettes, keyed exactly like `_lightPresetMap`.
pub fn light_preset_map() -> BTreeMap<&'static str, LightPalette> {
    let mut m: BTreeMap<&'static str, LightPalette> = BTreeMap::new();
    m.insert(
        "default",
        light_preset(
            "#F3F3F3", "#FFFFFF", "#F4F4F4", "#ECECEC", "#D6D6D6", "#E2E2E2", "#1F1F1F", "#666666",
            "#8A8A8A",
        ),
    );
    m.insert(
        "absolutely",
        light_preset(
            "#F5F3ED", "#FFFFFF", "#EFECE3", "#E4E0D5", "#D5CFC1", "#E6E1D4", "#32302C", "#686358",
            "#8E897D",
        ),
    );
    m.insert(
        "ayu",
        light_preset(
            "#F8F6F0", "#FFFFFF", "#F1EDE4", "#E8E0D2", "#D8D0C0", "#E6DED0", "#2F302A", "#68655D",
            "#8C877C",
        ),
    );
    m.insert(
        "catppuccin",
        light_preset(
            "#EFF1F5", "#FFFFFF", "#E9ECF2", "#DDE2EC", "#CAD0DC", "#DCE0E8", "#4C4F69", "#6C6F85",
            "#8C8FA1",
        ),
    );
    m.insert(
        "codex",
        light_preset(
            "#F5F6F8", "#FFFFFF", "#EAEEF5", "#DDE3ED", "#D6DCE7", "#E4E8F0", "#1C2430", "#5C6675",
            "#7C8796",
        ),
    );
    m.insert(
        "dracula",
        light_preset(
            "#F7F4FC", "#FFFFFF", "#F0EAF8", "#E6DEF0", "#D8CCE4", "#E7DEEF", "#2B2334", "#6C5D7E",
            "#9385A2",
        ),
    );
    m.insert(
        "everforest",
        light_preset(
            "#F7F5EC", "#FFFFFF", "#EFEBDD", "#E6DFCC", "#D5CAB4", "#E5DECD", "#3C4033", "#677151",
            "#8B9278",
        ),
    );
    m.insert(
        "github",
        light_preset(
            "#F6F8FA", "#FFFFFF", "#F0F3F6", "#EAEFF4", "#D0D7DE", "#D8DEE4", "#24292F", "#57606A",
            "#6E7781",
        ),
    );
    m.insert(
        "gruvbox",
        light_preset(
            "#F8F3E4", "#FFFFFF", "#EFE7D4", "#E4D9C2", "#D6C7AC", "#E7DCC8", "#3C3836", "#665C54",
            "#928374",
        ),
    );
    m.insert(
        "linear",
        light_preset(
            "#F7F7F8", "#FFFFFF", "#EEEFF4", "#E2E4EC", "#D8DAE4", "#E6E8EF", "#20242D", "#5F6675",
            "#858C9A",
        ),
    );
    m.insert(
        "lobster",
        light_preset(
            "#FAF3F5", "#FFFFFF", "#F3E7EB", "#EAD9DF", "#DBC4CC", "#E9D8DE", "#3B1D29", "#835466",
            "#A77A89",
        ),
    );
    m.insert(
        "material",
        light_preset(
            "#F1F4F6", "#FFFFFF", "#E9EEF2", "#DEE6EC", "#CAD4DC", "#E0E8ED", "#263238", "#546E7A",
            "#78909C",
        ),
    );
    m.insert(
        "matrix",
        light_preset(
            "#F1FAF1", "#FFFFFF", "#E7F2E7", "#DAE9DA", "#C4D8C4", "#DCEADC", "#0F2A13", "#2F6F3A",
            "#5D9567",
        ),
    );
    m.insert(
        "midnight",
        light_preset(
            "#F3F3F3", "#FFFFFF", "#F4F4F4", "#ECECEC", "#D6D6D6", "#E2E2E2", "#1F1F1F", "#666666",
            "#8A8A8A",
        ),
    );
    m.insert(
        "monokai",
        light_preset(
            "#F7F6EF", "#FFFFFF", "#EFEEE4", "#E5E2D4", "#D5CEB8", "#E6E1CF", "#272822", "#6F6B58",
            "#918C76",
        ),
    );
    m.insert(
        "night-owl",
        light_preset(
            "#F0F4F8", "#FAFDFF", "#E3EEF7", "#D7E5F0", "#C4D3DF", "#DAE6EF", "#183348", "#4D6477",
            "#768A9A",
        ),
    );
    m.insert(
        "nord",
        light_preset(
            "#ECEFF4", "#F7F9FC", "#E5E9F0", "#D8DEE9", "#C7CFDC", "#D8DEE9", "#2E3440", "#4C566A",
            "#6B7587",
        ),
    );
    m.insert(
        "notion",
        light_preset(
            "#FFFFFF", "#FBFBFA", "#F1F1EF", "#E9E9E7", "#D9D9D6", "#E7E7E4", "#37352F", "#6B6A66",
            "#9B9A97",
        ),
    );
    m.insert(
        "one",
        light_preset(
            "#FAFAFA", "#FFFFFF", "#EDEFF3", "#E0E4EA", "#D2D7E0", "#E2E5EA", "#383A42", "#6B7080",
            "#8D93A2",
        ),
    );
    m.insert(
        "oscurange",
        light_preset(
            "#FAF4EC", "#FFFFFF", "#F1E7DA", "#E7DAC9", "#D8C6AE", "#E8DBC9", "#3A2515", "#775437",
            "#9D7959",
        ),
    );
    m.insert(
        "raycast",
        light_preset(
            "#FFFFFF", "#FAFAFA", "#F0F0F0", "#E7E7E7", "#D8D8D8", "#E5E5E5", "#1F1F1F", "#666666",
            "#909090",
        ),
    );
    m.insert(
        "rose-pine",
        light_preset(
            "#FAF6F1", "#FFFFFF", "#F1EBE6", "#E8DED8", "#D8CAC4", "#E8DED8", "#575279", "#797593",
            "#9893A5",
        ),
    );
    m.insert(
        "sentry",
        light_preset(
            "#FAF7FF", "#FFFFFF", "#EFE7FA", "#E4D6F5", "#D4BEEB", "#E8DCF5", "#2F253B", "#6E5B84",
            "#9582A8",
        ),
    );
    m.insert(
        "solarized",
        light_preset(
            "#FAF5E6", "#FFFFFF", "#F0E9D6", "#E6DCC6", "#D7C9AE", "#E8DEC8", "#586E75", "#657B83",
            "#93A1A1",
        ),
    );
    m.insert(
        "temple",
        light_preset(
            "#F6FAEF", "#FFFFFF", "#EDF3E1", "#E3EBD4", "#CDD9B7", "#E1EBD3", "#26331B", "#657A3C",
            "#8B9D68",
        ),
    );
    m.insert(
        "tokyo-night",
        light_preset(
            "#D5D6DB", "#F5F6FA", "#E7E9F2", "#DDE0EA", "#C8CEDD", "#DDE0EA", "#343B58", "#565F89",
            "#7B829E",
        ),
    );
    m.insert(
        "vercel",
        light_preset(
            "#FFFFFF", "#FAFAFA", "#F0F0F0", "#E7E7E7", "#D8D8D8", "#E5E5E5", "#111111", "#666666",
            "#8C8C8C",
        ),
    );
    m.insert(
        "vscode-plus",
        light_preset(
            "#FFFFFF", "#FAFAFA", "#F0F3F7", "#E5EBF2", "#D0D7E2", "#E1E6ED", "#1E1E1E", "#5F6B7A",
            "#7F8996",
        ),
    );
    m.insert(
        "xcode",
        light_preset(
            "#FFFFFF", "#F8FAFD", "#EAF2FC", "#DCEAF8", "#C8D6E6", "#DDE7F2", "#1B2430", "#56677F",
            "#7C8DA3",
        ),
    );
    m
}

/// Curated accent palette for the Appearance Studio swatch grid.
pub const ACCENT_SWATCHES: [&str; 14] = [
    "#0078D4", "#0099BC", "#00B294", "#107C10", "#744DA9", "#BD93F9", "#CBA6F7", "#B146C2",
    "#E3008C", "#C30052", "#E81123", "#F7630C", "#FFB900", "#498205",
];

// ── Inputs ───────────────────────────────────────────────────────────────

/// The bound inputs Main.qml feeds into the Theme singleton.
#[derive(Clone, Debug)]
pub struct ThemeInputs {
    pub dark: bool,
    pub accent: Color32,
    pub density: String,
    pub corner_radius: i64,
    pub font_scale: f64,
    pub font_family_name: String,
    pub animations: bool,
    pub preset: String,
    pub base_tint: String,
    pub backdrop: String,
    pub transparency_strength: i64,
    pub backdrop_available: bool,
    pub wallpaper_active: bool,
}

impl Default for ThemeInputs {
    fn default() -> Self {
        Self {
            dark: false,
            accent: hex_color("#0078D4").expect("accent default"),
            density: "comfortable".into(),
            corner_radius: 7,
            font_scale: 1.0,
            font_family_name: "Segoe UI".into(),
            animations: true,
            preset: "default".into(),
            base_tint: String::new(),
            backdrop: "mica".into(),
            transparency_strength: 50,
            backdrop_available: true,
            wallpaper_active: false,
        }
    }
}

/// The resolved Theme singleton. Every QML `readonly property` token has a
/// field here (same names, snake_case).
pub struct Theme {
    // Driven inputs
    pub dark: bool,
    pub accent: Color32,
    pub backdrop: String,
    pub preset: String,
    pub base_tint: String,
    pub wallpaper_active: bool,
    pub backdrop_available: bool,

    // 2.0 derived flags
    pub is_translucent: bool,
    pub translucent_style: bool,
    pub amoled: bool,
    pub preset_colored: bool,

    // Accent derivatives
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub accent_text: Color32,
    pub accent_soft: Color32,

    // Surfaces
    pub bg: Color32,
    pub window_base: Color32,
    pub window_veil: Color32,
    pub content_panel: Color32,
    pub rail_panel: Color32,
    pub chrome_panel: Color32,
    pub bg_elevated: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub card_pressed: Color32,
    pub nav_hover: Color32,
    pub flyout: Color32,
    pub flyout_border: Color32,

    // Lines
    pub border_solid: Color32,
    pub divider: Color32,
    pub border_bottom: Color32,

    // Text
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,

    // Semantic
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub danger_fill: Color32,
    pub info: Color32,

    // Standard (non-accent) button fill
    pub control_fill: Color32,
    pub control_fill_hover: Color32,
    pub control_fill_pressed: Color32,

    // Elevation
    pub shadow_color: Color32,
    pub shadow_opacity: f64,
    pub shadow_blur: f64,
    pub shadow_v_offset: i32,

    // Geometry
    pub density_scale: f64,
    /// The raw fontScale input (`uiFontScale / 100`) used by the rail and
    /// title-bar metrics.
    pub font_scale: f64,
    pub radius: i32,
    pub radius_small: i32,
    pub spacing: i32,
    pub spacing_large: i32,
    pub rail_width: i32,
    pub rail_width_compact: i32,
    pub control_height: i32,

    // Typography
    pub font_family: String,
    pub font_small: i32,
    pub font_normal: i32,
    pub font_strong: i32,
    pub font_title: i32,
    pub font_hero: i32,

    // Motion
    pub anim_fast: i32,
    pub anim_normal: i32,
    pub anim_slow: i32,
}

impl Theme {
    /// `_accentIsLight`.
    pub fn accent_is_light(c: Color32) -> bool {
        let r = c.r() as f64 / 255.0;
        let g = c.g() as f64 / 255.0;
        let b = c.b() as f64 / 255.0;
        (r * 0.299 + g * 0.587 + b * 0.114) > 0.62
    }

    /// `_darkTint(baseTint)`: 30% of the tint over a 0x20 base.
    pub fn dark_tint(hex: &str) -> Color32 {
        let Some(base) = hex_color(hex) else {
            return hex_color("#000000").expect("black");
        };
        let mix = |c: u8| -> u8 {
            let v = (c as f64 * 0.30 + 0x20 as f64 * 0.70).round();
            v.clamp(0.0, 255.0) as u8
        };
        Color32::from_rgba_unmultiplied(mix(base.r()), mix(base.g()), mix(base.b()), 255)
    }

    /// `_lightTint(baseTint)`: 20% of the tint over white.
    pub fn light_tint(hex: &str) -> Color32 {
        let Some(base) = hex_color(hex) else {
            return hex_color("#FFFFFF").expect("white");
        };
        let mix = |c: u8| -> u8 {
            let v = (c as f64 * 0.20 + 0.80 * 255.0).round();
            v.clamp(0.0, 255.0) as u8
        };
        Color32::from_rgba_unmultiplied(mix(base.r()), mix(base.g()), mix(base.b()), 255)
    }

    /// Resolve every token from the current inputs (Main.qml bindings).
    pub fn resolve(input: &ThemeInputs) -> Theme {
        let dark = input.dark;
        let accent = input.accent;
        let backdrop = input.backdrop.clone();
        let dark_map = dark_preset_map();
        let preset = if dark_map.contains_key(input.preset.as_str()) {
            input.preset.clone()
        } else {
            "default".to_string()
        };
        let base_tint = input.base_tint.clone();
        let wallpaper_active = input.wallpaper_active;

        let dark_pal = *dark_map
            .get(preset.as_str())
            .unwrap_or(&dark_map["default"]);
        let light_map = light_preset_map();
        let light_pal = *light_map
            .get(preset.as_str())
            .unwrap_or(&light_map["default"]);

        let is_translucent = input.backdrop_available && backdrop != "solid";
        let translucent_style = true;
        let amoled = preset == "midnight";
        let preset_colored = dark_pal.colored
            || preset == "nord"
            || preset == "solarized"
            || preset == "dracula"
            || preset == "catppuccin"
            || (preset == "midnight" && dark);

        let pal_color = |name: &str, fallback: Color32| -> Color32 {
            let value = match name {
                "elevated" => dark_pal.elevated,
                "card" => dark_pal.card,
                "cardHover" => dark_pal.card_hover,
                "cardPressed" => dark_pal.card_pressed,
                "flyout" => dark_pal.flyout,
                "flyoutBorder" => dark_pal.flyout_border,
                "border" => dark_pal.border,
                "divider" => dark_pal.divider,
                "text" => dark_pal.text,
                "muted" => dark_pal.muted,
                "faint" => dark_pal.faint,
                "control" => dark_pal.control,
                "controlHover" => dark_pal.control_hover,
                "controlPressed" => dark_pal.control_pressed,
                _ => None,
            };
            value.and_then(hex_color).unwrap_or(fallback)
        };

        let hex_with_alpha = |hex: &str, alpha: f64| -> Color32 {
            match hex_color(hex) {
                Some(c) if hex.len() >= 7 => with_alpha(c, alpha),
                Some(c) => c,
                None => Color32::TRANSPARENT,
            }
        };

        let pal_layer = |name: &str, fallback: Color32, alpha: f64| -> Color32 {
            if !(dark && preset_colored) {
                return pal_color(name, fallback);
            }
            let value = match name {
                "card" => dark_pal.card,
                "cardHover" => dark_pal.card_hover,
                "cardPressed" => dark_pal.card_pressed,
                "flyout" => dark_pal.flyout,
                "flyoutBorder" => dark_pal.flyout_border,
                "border" => dark_pal.border,
                "divider" => dark_pal.divider,
                "text" => dark_pal.text,
                "muted" => dark_pal.muted,
                "faint" => dark_pal.faint,
                "control" => dark_pal.control,
                "controlHover" => dark_pal.control_hover,
                "controlPressed" => dark_pal.control_pressed,
                "elevated" => dark_pal.elevated,
                _ => None,
            };
            match value {
                Some(v) if !v.is_empty() => {
                    if translucent_style {
                        hex_with_alpha(v, alpha)
                    } else {
                        hex_color(v).unwrap_or(fallback)
                    }
                }
                _ => fallback,
            }
        };

        let light_color = |name: &str, fallback: Color32| -> Color32 {
            let value = match name {
                "bg" => Some(light_pal.bg),
                "layer" => Some(light_pal.layer),
                "hover" => Some(light_pal.hover),
                "press" => Some(light_pal.press),
                "border" => Some(light_pal.border),
                "divider" => Some(light_pal.divider),
                "text" => Some(light_pal.text),
                "muted" => Some(light_pal.muted),
                "faint" => Some(light_pal.faint),
                _ => None,
            };
            value.and_then(hex_color).unwrap_or(fallback)
        };

        let light_layer = |name: &str, fallback: Color32, alpha: f64| -> Color32 {
            if !preset_colored {
                return fallback;
            }
            let value = match name {
                "bg" => Some(light_pal.bg),
                "layer" => Some(light_pal.layer),
                "hover" => Some(light_pal.hover),
                "press" => Some(light_pal.press),
                "border" => Some(light_pal.border),
                "divider" => Some(light_pal.divider),
                "text" => Some(light_pal.text),
                "muted" => Some(light_pal.muted),
                "faint" => Some(light_pal.faint),
                _ => None,
            };
            match value {
                Some(v) if !v.is_empty() => {
                    if translucent_style {
                        hex_with_alpha(v, alpha)
                    } else {
                        hex_color(v).unwrap_or(fallback)
                    }
                }
                _ => fallback,
            }
        };

        let transparency_amount = input.transparency_strength.clamp(0, 100) as f64 / 100.0;
        let transparency_alpha_factor = 1.45 - transparency_amount * 1.05;
        let dark_mica_veil_alpha = 0.82 - transparency_amount * 0.60;
        let light_mica_veil_alpha = 0.70 - transparency_amount * 0.54;
        let dark_palette_window_alpha = 0.88_f64.min(0.56 * transparency_alpha_factor);
        let light_palette_window_alpha = 0.72_f64.min(0.34 * transparency_alpha_factor);

        let solid_backdrop_base = if dark {
            match dark_pal.win {
                Some(v) if !v.is_empty() => {
                    hex_color(v).unwrap_or_else(|| hex_color(dark_pal.d0).expect("d0"))
                }
                _ => hex_color(dark_pal.d0).expect("d0"),
            }
        } else {
            light_color("bg", hex_color(light_pal.bg).expect("light bg"))
        };

        let over_opaque = |fore: Color32, back: Color32| -> Color32 {
            let a = fore.a() as f64 / 255.0;
            blend(fore, back, a)
        };

        let fallback_material = if dark {
            hex_color(dark_map["default"].d0).expect("default d0")
        } else {
            hex_color(dark_map["default"].lt).expect("default lt")
        };

        // windowVeil
        let window_veil = if !base_tint.is_empty() {
            if dark {
                Self::dark_tint(&base_tint)
            } else {
                Self::light_tint(&base_tint)
            }
        } else if preset_colored {
            if dark {
                if backdrop != "solid" {
                    let base = match dark_pal.win {
                        Some(v) if !v.is_empty() => v,
                        _ => dark_pal.d0,
                    };
                    hex_with_alpha(base, dark_palette_window_alpha)
                } else {
                    solid_backdrop_base
                }
            } else if backdrop != "solid" {
                hex_with_alpha(dark_pal.lt, light_palette_window_alpha)
            } else {
                solid_backdrop_base
            }
        } else if backdrop != "solid" {
            if dark {
                hex_with_alpha(dark_pal.d0, dark_mica_veil_alpha)
            } else {
                hex_with_alpha(light_pal.bg, light_mica_veil_alpha)
            }
        } else {
            solid_backdrop_base
        };

        let window_base =
            if !input.backdrop_available && backdrop != "solid" && base_tint.is_empty() {
                over_opaque(window_veil, fallback_material)
            } else {
                window_veil
            };

        // Surfaces
        let content_panel = if dark {
            if translucent_style {
                pal_layer(
                    "card",
                    qt_rgba(1.0, 1.0, 1.0, 0.048),
                    0.44 * transparency_alpha_factor,
                )
            } else {
                pal_color("card", hex_color(dark_pal.d1).expect("d1"))
            }
        } else if translucent_style {
            if wallpaper_active {
                qt_rgba(1.0, 1.0, 1.0, 0.18)
            } else {
                light_layer(
                    "bg",
                    qt_rgba(1.0, 1.0, 1.0, 0.18),
                    0.32 * transparency_alpha_factor,
                )
            }
        } else {
            light_color("layer", hex_color("#FFFFFF").expect("white"))
        };

        let rail_panel = if dark {
            if translucent_style {
                pal_layer(
                    "control",
                    qt_rgba(1.0, 1.0, 1.0, 0.065),
                    0.72 * transparency_alpha_factor,
                )
            } else {
                pal_color("control", hex_color(dark_pal.d1).expect("d1"))
            }
        } else if translucent_style {
            if wallpaper_active {
                qt_rgba(1.0, 1.0, 1.0, 0.24)
            } else {
                light_layer(
                    "press",
                    qt_rgba(1.0, 1.0, 1.0, 0.34),
                    0.70 * transparency_alpha_factor,
                )
            }
        } else {
            light_color("layer", hex_color("#FFFFFF").expect("white"))
        };

        let chrome_panel = if dark {
            pal_layer(
                "control",
                qt_rgba(1.0, 1.0, 1.0, 0.030),
                0.26 * transparency_alpha_factor,
            )
        } else if wallpaper_active {
            qt_rgba(1.0, 1.0, 1.0, 0.12)
        } else {
            qt_rgba(1.0, 1.0, 1.0, 0.22 * transparency_alpha_factor)
        };

        let bg_elevated = if dark {
            if translucent_style {
                qt_rgba(1.0, 1.0, 1.0, 0.016)
            } else {
                pal_color("elevated", hex_color(dark_pal.d1).expect("d1"))
            }
        } else if translucent_style {
            if wallpaper_active {
                qt_rgba(1.0, 1.0, 1.0, 0.28)
            } else {
                qt_rgba(1.0, 1.0, 1.0, 0.55)
            }
        } else {
            light_color("layer", hex_color("#FFFFFF").expect("white"))
        };

        let card = if dark {
            if translucent_style {
                qt_rgba(1.0, 1.0, 1.0, 0.040)
            } else {
                pal_color("card", hex_color(dark_pal.d1).expect("d1"))
            }
        } else if translucent_style {
            if wallpaper_active {
                qt_rgba(1.0, 1.0, 1.0, 0.22)
            } else {
                qt_rgba(1.0, 1.0, 1.0, 0.38)
            }
        } else {
            light_color("layer", hex_color("#FFFFFF").expect("white"))
        };

        let card_hover = if dark {
            if translucent_style {
                qt_rgba(1.0, 1.0, 1.0, 0.0837)
            } else {
                pal_color(
                    "cardHover",
                    qml_lighter(hex_color(dark_pal.d1).expect("d1"), 1.12),
                )
            }
        } else if translucent_style {
            if wallpaper_active {
                qt_rgba(1.0, 1.0, 1.0, 0.32)
            } else {
                qt_rgba(1.0, 1.0, 1.0, 0.48)
            }
        } else {
            light_color("hover", hex_color("#F4F4F4").expect("hover"))
        };

        let card_pressed = if dark {
            if translucent_style {
                qt_rgba(1.0, 1.0, 1.0, 0.0326)
            } else {
                pal_color("cardPressed", hex_color(dark_pal.d0).expect("d0"))
            }
        } else if translucent_style {
            if wallpaper_active {
                qt_rgba(1.0, 1.0, 1.0, 0.40)
            } else {
                qt_rgba(1.0, 1.0, 1.0, 0.56)
            }
        } else {
            light_color("press", hex_color("#ECECEC").expect("press"))
        };

        let accent_soft = if dark {
            with_alpha(accent, 0.22)
        } else {
            with_alpha(accent, 0.20)
        };

        let nav_hover = if dark {
            card_hover
        } else {
            with_alpha(accent, 0.11)
        };

        let flyout = if dark {
            hex_color(dark_pal.d1).expect("d1")
        } else {
            hex_color("#FBFBFB").expect("flyout")
        };

        let text = if dark {
            pal_color("text", qt_rgba(1.0, 1.0, 1.0, 1.0))
        } else {
            let base = light_color("text", qt_rgba(0.0, 0.0, 0.0, 0.89));
            if wallpaper_active {
                qml_darker(base, 1.18)
            } else {
                base
            }
        };
        let text_muted = if dark {
            pal_color("muted", qt_rgba(1.0, 1.0, 1.0, 0.79))
        } else {
            let base = light_color("muted", qt_rgba(0.0, 0.0, 0.0, 0.61));
            if wallpaper_active {
                qml_darker(base, 1.30)
            } else {
                base
            }
        };
        let text_faint = if dark {
            pal_color("faint", qt_rgba(1.0, 1.0, 1.0, 0.55))
        } else {
            let base = light_color("faint", qt_rgba(0.0, 0.0, 0.0, 0.45));
            if wallpaper_active {
                qml_darker(base, 1.42)
            } else {
                base
            }
        };

        let control_fill = if dark {
            if translucent_style {
                qt_rgba(1.0, 1.0, 1.0, 0.06)
            } else {
                pal_color("control", hex_color(dark_pal.d1).expect("d1"))
            }
        } else if translucent_style {
            qt_rgba(1.0, 1.0, 1.0, 0.70)
        } else {
            hex_color("#FFFFFF").expect("white")
        };
        let control_fill_hover = if dark {
            if translucent_style {
                qt_rgba(1.0, 1.0, 1.0, 0.08)
            } else {
                pal_color(
                    "controlHover",
                    qml_lighter(hex_color(dark_pal.d1).expect("d1"), 1.12),
                )
            }
        } else if translucent_style {
            qt_rgba(249.0 / 255.0, 249.0 / 255.0, 249.0 / 255.0, 0.5)
        } else {
            hex_color("#F4F4F4").expect("hover")
        };
        let control_fill_pressed = if dark {
            if translucent_style {
                qt_rgba(1.0, 1.0, 1.0, 0.03)
            } else {
                pal_color("controlPressed", hex_color(dark_pal.d0).expect("d0"))
            }
        } else if translucent_style {
            qt_rgba(249.0 / 255.0, 249.0 / 255.0, 249.0 / 255.0, 0.3)
        } else {
            hex_color("#ECECEC").expect("pressed")
        };

        // Geometry
        let font_scale = input.font_scale;
        let density = input.density.as_str();
        let round_scaled = |base: f64| (base * font_scale).round() as i32;
        let spacing = round_scaled(if density == "compact" {
            8.0
        } else if density == "spacious" {
            16.0
        } else {
            12.0
        });
        let spacing_large = round_scaled(if density == "compact" {
            14.0
        } else if density == "spacious" {
            26.0
        } else {
            20.0
        });
        let radius = input.corner_radius.clamp(0, 20) as i32;
        let radius_small = (radius - 3).max(2);
        let control_height = round_scaled(if density == "compact" {
            28.0
        } else if density == "spacious" {
            38.0
        } else {
            32.0
        });

        let font_family = if input.font_family_name.is_empty() {
            "Segoe UI".to_string()
        } else {
            input.font_family_name.clone()
        };

        Theme {
            dark,
            accent,
            backdrop,
            preset,
            base_tint,
            wallpaper_active,
            backdrop_available: input.backdrop_available,
            is_translucent,
            translucent_style,
            amoled,
            preset_colored,
            accent_hover: if dark {
                qml_lighter(accent, 1.12)
            } else {
                qml_darker(accent, 1.06)
            },
            accent_pressed: if dark {
                qml_lighter(accent, 1.22)
            } else {
                qml_darker(accent, 1.14)
            },
            accent_text: if Self::accent_is_light(accent) {
                hex_color("#111111").expect("accent text")
            } else {
                hex_color("#FFFFFF").expect("accent text")
            },
            accent_soft,
            bg: if dark {
                hex_color(dark_pal.d0).expect("d0")
            } else {
                light_color("bg", hex_color(light_pal.bg).expect("light bg"))
            },
            window_base,
            window_veil,
            content_panel,
            rail_panel,
            chrome_panel,
            bg_elevated,
            card,
            card_hover,
            card_pressed,
            nav_hover,
            flyout,
            flyout_border: if dark {
                qt_rgba(1.0, 1.0, 1.0, 0.10)
            } else {
                qt_rgba(0.0, 0.0, 0.0, 0.13)
            },
            border_solid: if dark {
                qt_rgba(1.0, 1.0, 1.0, 0.09)
            } else {
                qt_rgba(0.0, 0.0, 0.0, 0.07)
            },
            divider: if dark {
                qt_rgba(1.0, 1.0, 1.0, 0.06)
            } else {
                qt_rgba(0.0, 0.0, 0.0, 0.06)
            },
            border_bottom: if dark {
                qt_rgba(0.0, 0.0, 0.0, 0.20)
            } else {
                qt_rgba(0.0, 0.0, 0.0, 0.10)
            },
            text,
            text_muted,
            text_faint,
            success: hex_color(if dark { "#6CCB5F" } else { "#3BAE45" }).expect("success"),
            warning: hex_color(if dark { "#FCE100" } else { "#F4C900" }).expect("warning"),
            danger: hex_color(if dark { "#FF99A4" } else { "#B84A4A" }).expect("danger"),
            danger_fill: hex_color(if dark { "#D13438" } else { "#D65A5A" }).expect("danger fill"),
            info: accent,
            control_fill,
            control_fill_hover,
            control_fill_pressed,
            shadow_color: Color32::BLACK,
            shadow_opacity: if dark { 0.44 } else { 0.16 },
            shadow_blur: 0.85,
            shadow_v_offset: 5,
            density_scale: if density == "compact" {
                0.85
            } else if density == "spacious" {
                1.18
            } else {
                1.0
            },
            font_scale,
            radius,
            radius_small,
            spacing,
            spacing_large,
            rail_width: round_scaled(250.0),
            rail_width_compact: round_scaled(50.0),
            control_height,
            font_family,
            font_small: round_scaled(12.0),
            font_normal: round_scaled(14.0),
            font_strong: round_scaled(14.0),
            font_title: round_scaled(20.0),
            font_hero: round_scaled(28.0),
            anim_fast: if input.animations { 110 } else { 0 },
            anim_normal: if input.animations { 190 } else { 0 },
            anim_slow: if input.animations { 320 } else { 0 },
        }
    }

    /// `pingColor(ping)`.
    pub fn ping_color(&self, ping: Option<i64>) -> Color32 {
        match ping {
            None => self.text_faint,
            Some(v) if v < 0 => self.text_faint,
            Some(v) if v < 120 => self.success,
            Some(v) if v < 250 => self.warning,
            Some(_) => self.danger,
        }
    }

    /// `formatRate(bps)` — Russian units byte-identical to Theme.qml.
    pub fn format_rate(bps: f64) -> String {
        let mut v = bps;
        if !v.is_finite() || v < 0.0 {
            v = 0.0;
        }
        if v >= 1024.0 * 1024.0 * 1024.0 {
            return format!(
                "{:.2} \u{0413}\u{0431}/\u{0441}",
                v / (1024.0 * 1024.0 * 1024.0)
            );
        }
        if v >= 1024.0 * 1024.0 {
            return format!("{:.2} \u{041C}\u{0431}/\u{0441}", v / (1024.0 * 1024.0));
        }
        if v >= 1024.0 {
            return format!("{:.1} \u{041A}\u{0431}/\u{0441}", v / 1024.0);
        }
        format!("{} \u{0431}/\u{0441}", v.round() as i64)
    }
}

// ── Fonts ────────────────────────────────────────────────────────────────

/// Installed UI font set (loaded once; egui renders from these).
struct LoadedFonts {
    definitions: FontDefinitions,
}

static LOADED_FONTS: std::sync::OnceLock<LoadedFonts> = std::sync::OnceLock::new();

fn read_font_file(path: &PathBuf) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Candidate Segoe Fluent Icons sources, mirroring `_load_bundled_fonts`:
/// the Windows 11 system icon font first, then the asset bundled next to the
/// executable (the Python app ships `assets/fonts/SegoeIcons.ttf`).
fn icon_font_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(windows) {
        if let Some(windir) = std::env::var_os("WINDIR") {
            let fonts = PathBuf::from(windir).join("Fonts");
            paths.push(fonts.join("segflui.ttf"));
        }
    }
    paths.push(
        lumen_core::constants::base_dir()
            .join("assets")
            .join("fonts")
            .join("SegoeIcons.ttf"),
    );
    paths
}

fn ui_font_candidates(family: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(windows) {
        if let Some(windir) = std::env::var_os("WINDIR") {
            let fonts = PathBuf::from(windir).join("Fonts");
            if family != "Segoe UI" && !family.is_empty() {
                let compact = family.replace(' ', "");
                paths.push(fonts.join(format!("{compact}.ttf")));
                paths.push(fonts.join(format!("{compact}-Regular.ttf")));
            }
            paths.push(fonts.join("segoeui.ttf"));
            paths.push(fonts.join("segoeuib.ttf"));
            paths.push(fonts.join("segoeuisb.ttf"));
        }
    }
    paths
}

fn load_fonts_once(family: &str) -> &'static LoadedFonts {
    LOADED_FONTS.get_or_init(|| {
        let mut definitions = FontDefinitions::default();
        let mut proportional: Vec<String> = Vec::new();

        // Register each readable candidate under its file stem. The first
        // readable regular-weight file wins the primary slot; bolder variants
        // follow as later fallbacks (egui resolves every glyph from the first
        // font that covers it).
        for path in ui_font_candidates(family) {
            if let Some(data) = read_font_file(&path) {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "UI".to_string());
                if definitions.font_data.contains_key(&name) {
                    continue;
                }
                definitions
                    .font_data
                    .insert(name.clone(), FontData::from_owned(data).into());
                proportional.push(name.clone());
            }
        }
        if proportional.is_empty() {
            // Keep egui's shipped fonts when nothing loads from the system.
            proportional.push("Ubuntu-Light".into());
        }
        // Keep the shipped emoji fonts as later fallbacks when present.
        for family_name in ["NotoEmoji-Regular", "emoji-icon-font"] {
            if definitions.font_data.contains_key(family_name) {
                proportional.push(family_name.to_string());
            }
        }

        let icon_font = icon_font_candidates().iter().find_map(read_font_file)
            .map_or_else(
                || FontData::from_static(include_bytes!(
                    "../../../../../windows/xray_fluent/qml_app/assets/fonts/SegoeIcons.ttf"
                )),
                FontData::from_owned,
            );
        definitions.font_data.insert("Segoe Fluent Icons".into(), icon_font.into());

        // The icon font supplies the Private Use Area glyphs used by QML.
        proportional.push("Segoe Fluent Icons".into());
        for (family, stem) in [("Segoe UI Semibold", "segoeuisb"), ("Segoe UI Bold", "segoeuib")] {
            let mut fonts = proportional.clone();
            if let Some(index) = fonts.iter().position(|name| name == stem) {
                fonts[..=index].rotate_right(1);
            }
            definitions.families.insert(FontFamily::Name(family.into()), fonts);
        }
        definitions
            .families
            .insert(FontFamily::Proportional, proportional);
        if let Some(font) = std::env::var_os("WINDIR")
            .and_then(|root| read_font_file(&PathBuf::from(root).join("Fonts").join("consola.ttf")))
        {
            definitions.font_data.insert("Consolas".into(), FontData::from_owned(font).into());
            definitions.families.entry(FontFamily::Monospace).or_default().insert(0, "Consolas".into());
        }
        LoadedFonts { definitions }
    })
}

pub(super) fn semibold(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name("Segoe UI Semibold".into()))
}

pub(super) fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name("Segoe UI Bold".into()))
}

/// Install the Segoe UI + Segoe Fluent Icons fonts once per session and keep
/// text style sizes bound to the theme (`fontSmall`..`fontHero`).
pub fn apply_fonts(ctx: &Context, theme: &Theme) {
    let loaded = load_fonts_once(&theme.font_family);
    ctx.set_fonts(loaded.definitions.clone());
    ctx.global_style_mut(|style| {
        let font = egui::FontId::proportional(theme.font_normal as f32);
        style.text_styles = [
            (
                egui::TextStyle::Small,
                egui::FontId::proportional(theme.font_small as f32),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::proportional(theme.font_normal as f32),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::proportional(theme.font_normal as f32),
            ),
            (
                egui::TextStyle::Heading,
                egui::FontId::proportional(theme.font_hero as f32),
            ),
            (
                egui::TextStyle::Monospace,
                egui::FontId::monospace(theme.font_normal as f32),
            ),
            (egui::TextStyle::Name("Caption".into()), font.clone()),
            (
                egui::TextStyle::Name("Subtitle".into()),
                egui::FontId::proportional(theme.font_title as f32),
            ),
            (
                egui::TextStyle::Name("Strong".into()),
                egui::FontId::proportional(theme.font_strong as f32),
            ),
        ]
        .into_iter()
        .collect();
    });
}

/// Apply the resolved tokens to egui's visuals so widgets painted by pages
/// inherit the Fluent palette.
pub fn apply_visuals(ctx: &Context, theme: &Theme) {
    let mut style = Style::clone(&ctx.global_style());
    style.visuals.dark_mode = theme.dark;
    style.visuals.override_text_color = Some(theme.text);
    style.visuals.selection.bg_fill = theme.accent_soft;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, theme.accent);
    style.visuals.widgets.noninteractive.bg_fill = theme.bg_elevated;
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, theme.text_muted);
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, theme.border_solid);
    style.visuals.widgets.inactive.bg_fill = theme.control_fill;
    style.visuals.widgets.inactive.weak_bg_fill = theme.control_fill;
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, theme.text);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, theme.border_solid);
    style.visuals.widgets.hovered.weak_bg_fill = theme.control_fill_hover;
    style.visuals.widgets.hovered.bg_fill = theme.control_fill_hover;
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, theme.text);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, theme.border_solid);
    style.visuals.widgets.active.weak_bg_fill = theme.control_fill_pressed;
    style.visuals.widgets.active.bg_fill = theme.control_fill_pressed;
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, theme.text);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, theme.accent);
    style.visuals.window_fill = theme.flyout;
    style.visuals.window_stroke = egui::Stroke::new(1.0, theme.flyout_border);
    style.visuals.panel_fill = theme.rail_panel;
    style.visuals.extreme_bg_color = theme.bg;
    style.visuals.faint_bg_color = theme.card_pressed;
    style.visuals.window_shadow = egui::epaint::Shadow {
        color: Color32::from_black_alpha((theme.shadow_opacity * 255.0).round() as u8),
        offset: [0, theme.shadow_v_offset as i8],
        blur: (theme.shadow_blur * 32.0).round() as u8,
        spread: 0,
    };
    style.spacing.item_spacing = Vec2::splat(theme.spacing as f32);
    style.spacing.button_padding = Vec2::new(theme.spacing as f32, theme.spacing as f32 / 2.0);
    ctx.set_global_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tokens_match_theme_qml() {
        let theme = Theme::resolve(&ThemeInputs::default());
        assert!(!theme.dark);
        assert_eq!(theme.bg, hex_color("#F3F3F3").unwrap());
        assert_eq!(theme.accent, hex_color("#0078D4").unwrap());
        assert_eq!(theme.radius, 7);
        assert_eq!(theme.radius_small, 4);
        assert_eq!(theme.spacing, 12);
        assert_eq!(theme.spacing_large, 20);
        assert_eq!(theme.rail_width, 250);
        assert_eq!(theme.rail_width_compact, 50);
        assert_eq!(theme.control_height, 32);
        assert_eq!(theme.font_small, 12);
        assert_eq!(theme.font_normal, 14);
        assert_eq!(theme.font_title, 20);
        assert_eq!(theme.font_hero, 28);
        assert_eq!(theme.anim_fast, 110);
        assert!(!theme.preset_colored);
        assert!(theme.is_translucent);
        assert!(!theme.amoled);
    }

    #[test]
    fn accent_derivatives_use_qt_hsv_math() {
        let accent = hex_color("#0078D4").unwrap();
        let theme = Theme::resolve(&ThemeInputs::default());
        // Light theme: Qt.darker(accent, 1.06) -> val * 100/106.
        let (h, s, v) = rgb_to_hsv(
            accent.r() as f64 / 255.0,
            accent.g() as f64 / 255.0,
            accent.b() as f64 / 255.0,
        );
        let (_, _, vd) = rgb_to_hsv(
            theme.accent_hover.r() as f64 / 255.0,
            theme.accent_hover.g() as f64 / 255.0,
            theme.accent_hover.b() as f64 / 255.0,
        );
        assert!((vd - (v * 100.0 / 106.0)).abs() < 1.0 / 255.0);
        assert!((h - h).abs() < f64::EPSILON); // hue unchanged
        let _ = s;
        assert_eq!(
            theme.accent_text,
            Color32::from_rgba_unmultiplied(255, 255, 255, 255)
        );
    }

    #[test]
    fn accent_is_light_threshold_matches_qml() {
        assert!(Theme::accent_is_light(hex_color("#FFFFFF").unwrap()));
        assert!(Theme::accent_is_light(hex_color("#FFB900").unwrap()));
        assert!(!Theme::accent_is_light(hex_color("#0078D4").unwrap()));
    }

    #[test]
    fn tint_helpers_match_theme_qml() {
        let tinted = Theme::light_tint("#0078D4");
        assert_eq!(
            tinted,
            Color32::from_rgba_unmultiplied(
                ((0x00 as f64 * 0.20 + 0.80 * 255.0).round()) as u8,
                ((0x78 as f64 * 0.20 + 0.80 * 255.0).round()) as u8,
                ((0xD4 as f64 * 0.20 + 0.80 * 255.0).round()) as u8,
                255
            )
        );
        let dark_tinted = Theme::dark_tint("#FFFFFF");
        assert_eq!(
            dark_tinted,
            Color32::from_rgba_unmultiplied(
                ((255.0_f64 * 0.30 + 32.0 * 0.70).round()) as u8,
                ((255.0_f64 * 0.30 + 32.0 * 0.70).round()) as u8,
                ((255.0_f64 * 0.30 + 32.0 * 0.70).round()) as u8,
                255
            )
        );
    }

    #[test]
    fn transparency_inputs_scale_veil_alphas() {
        let mut input = ThemeInputs::default();
        input.transparency_strength = 100;
        input.dark = true;
        let theme = Theme::resolve(&input);
        // darkMicaVeilAlpha = 0.82 - 1.0*0.60 = 0.22 applied to #202020.
        assert_eq!(
            theme.window_veil,
            Color32::from_rgba_unmultiplied(0x20, 0x20, 0x20, (0.22_f64 * 255.0).round() as u8)
        );
    }

    #[test]
    fn presets_keep_their_named_surfaces() {
        let mut input = ThemeInputs::default();
        input.dark = true;
        input.preset = "dracula".into();
        let theme = Theme::resolve(&input);
        assert!(theme.preset_colored);
        assert_eq!(theme.bg, hex_color("#282A36").unwrap());
        assert_eq!(theme.text, hex_color("#F8F8F2").unwrap());
        assert_eq!(theme.flyout, hex_color("#343746").unwrap());

        input.preset = "nonexistent".into();
        let fallback = Theme::resolve(&input);
        assert_eq!(fallback.bg, hex_color("#202020").unwrap());
        assert!(!fallback.preset_colored);

        input.preset = "midnight".into();
        let midnight = Theme::resolve(&input);
        assert!(midnight.amoled);
        assert!(midnight.preset_colored);
    }

    #[test]
    fn light_theme_uses_the_light_palette() {
        let mut input = ThemeInputs::default();
        input.dark = false;
        input.preset = "github".into();
        let theme = Theme::resolve(&input);
        assert_eq!(theme.bg, hex_color("#F6F8FA").unwrap());
        assert_eq!(theme.text, hex_color("#24292F").unwrap());
    }

    #[test]
    fn ping_color_and_format_rate_match_theme_qml() {
        let theme = Theme::resolve(&ThemeInputs::default());
        assert_eq!(theme.ping_color(None), theme.text_faint);
        assert_eq!(theme.ping_color(Some(-1)), theme.text_faint);
        assert_eq!(theme.ping_color(Some(10)), theme.success);
        assert_eq!(theme.ping_color(Some(120)), theme.warning);
        assert_eq!(theme.ping_color(Some(250)), theme.danger);
        assert_eq!(Theme::format_rate(-5.0), "0 \u{0431}/\u{0441}");
        assert_eq!(Theme::format_rate(523.0), "523 \u{0431}/\u{0441}");
        assert_eq!(Theme::format_rate(2048.0), "2.0 \u{041A}\u{0431}/\u{0441}");
        assert_eq!(
            Theme::format_rate(3.5 * 1024.0 * 1024.0),
            "3.50 \u{041C}\u{0431}/\u{0441}"
        );
        assert_eq!(
            Theme::format_rate(2.75 * 1024.0 * 1024.0 * 1024.0),
            "2.75 \u{0413}\u{0431}/\u{0441}"
        );
    }

    #[test]
    fn font_scale_multiplies_geometry_and_type() {
        let mut input = ThemeInputs::default();
        input.font_scale = 1.25;
        let theme = Theme::resolve(&input);
        assert_eq!(theme.rail_width, 313); // round(250 * 1.25)
        assert_eq!(theme.font_normal, 18); // round(14 * 1.25)
        assert_eq!(theme.spacing, 15); // round(12 * 1.25)
        input.font_scale = 0.5;
        let half = Theme::resolve(&input);
        assert_eq!(half.rail_width, 125);
        assert_eq!(half.font_hero, 14);
    }

    #[test]
    fn compact_and_spacious_densities_match_qml() {
        let mut input = ThemeInputs::default();
        input.density = "compact".into();
        let compact = Theme::resolve(&input);
        assert_eq!(compact.spacing, 8);
        assert_eq!(compact.spacing_large, 14);
        assert_eq!(compact.control_height, 28);
        assert_eq!(compact.density_scale, 0.85);
        input.density = "spacious".into();
        let spacious = Theme::resolve(&input);
        assert_eq!(spacious.spacing, 16);
        assert_eq!(spacious.spacing_large, 26);
        assert_eq!(spacious.control_height, 38);
        assert_eq!(spacious.density_scale, 1.18);
    }

    #[test]
    fn accent_swatches_are_preserved() {
        assert_eq!(ACCENT_SWATCHES[0], "#0078D4");
        assert_eq!(ACCENT_SWATCHES.len(), 14);
        for swatch in ACCENT_SWATCHES {
            assert!(hex_color(swatch).is_some(), "invalid swatch {swatch}");
        }
    }

    #[test]
    fn base_tint_overrides_the_window_veil() {
        let mut input = ThemeInputs::default();
        input.dark = true;
        input.base_tint = "#123456".into();
        let theme = Theme::resolve(&input);
        assert_eq!(theme.window_base, Theme::dark_tint("#123456"));
    }

    #[test]
    fn qcolor_lighter_reduces_saturation_like_qt() {
        // QColor::lighter(150) of a saturated red keeps hue, raises value,
        // reduces saturation by (factor-100)/100 of the original.
        let red = Color32::from_rgba_unmultiplied(255, 0, 0, 255);
        let lighter = qcolor_lighter(red, 150);
        let (_, s, _) = rgb_to_hsv(
            lighter.r() as f64 / 255.0,
            lighter.g() as f64 / 255.0,
            lighter.b() as f64 / 255.0,
        );
        assert!((s - 0.5).abs() <= 1.0 / 255.0);
        assert_eq!(
            qcolor_lighter(Color32::from_rgb(100, 0, 0), 150),
            Color32::from_rgb(150, 0, 0)
        );
    }
}
