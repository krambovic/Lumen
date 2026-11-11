//! Navigation model (port of the `nav` list, `NavButton.qml` behavior and the
//! page switching state in `Main.qml`).

use egui::{Color32, Rect, Sense, Ui, Vec2};

use lumen_core::i18n::tr;

use super::theme::Theme;

/// Ordered page entries. Index == the `currentIndex` used by every page
/// agent, identical to the QML StackLayout child order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Dashboard = 0,
    Nodes = 1,
    Routing = 2,
    Configs = 3,
    Zapret = 4,
    Logs = 5,
    History = 6,
    Updates = 7,
    Settings = 8,
    About = 9,
}

impl Page {
    pub const ALL: [Page; 10] = [
        Page::Dashboard,
        Page::Nodes,
        Page::Routing,
        Page::Configs,
        Page::Zapret,
        Page::Logs,
        Page::History,
        Page::Updates,
        Page::Settings,
        Page::About,
    ];

    pub fn from_index(index: usize) -> Page {
        Page::ALL.get(index).copied().unwrap_or(Page::Dashboard)
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The localized page title (used by the placeholder page panels until the
    /// page agents land).
    pub fn title(self) -> &'static str {
        match self {
            Page::Dashboard => "\u{041F}\u{0430}\u{043D}\u{0435}\u{043B}\u{044C}",
            Page::Nodes => "\u{0421}\u{0435}\u{0440}\u{0432}\u{0435}\u{0440}\u{044B}",
            Page::Routing => "\u{041C}\u{0430}\u{0440}\u{0448}\u{0440}\u{0443}\u{0442}\u{044B}",
            Page::Configs => "Configs",
            Page::Zapret => "Zapret",
            Page::Logs => "\u{041B}\u{043E}\u{0433}\u{0438}",
            Page::History => "\u{0418}\u{0441}\u{0442}\u{043E}\u{0440}\u{0438}\u{044F}",
            Page::Updates => {
                "\u{041E}\u{0431}\u{043D}\u{043E}\u{0432}\u{043B}\u{0435}\u{043D}\u{0438}\u{044F}"
            }
            Page::Settings => {
                "\u{041D}\u{0430}\u{0441}\u{0442}\u{0440}\u{043E}\u{0439}\u{043A}\u{0438}"
            }
            Page::About => "\u{041E} \u{043F}\u{0440}\u{043E}\u{0435}\u{043A}\u{0442}\u{0435}",
        }
    }
}

/// Which half of the rail the entry renders in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Top,
    Bottom,
}

/// One row of the Main.qml `nav` array.
pub struct NavEntry {
    pub page: Page,
    /// Raw i18n key (Main.qml passes labels through `I18n.t`).
    pub label_key: &'static str,
    /// Segoe Fluent Icons glyph, kept byte-identical.
    pub glyph: &'static str,
    /// SVG icon name for entries that draw an outline path (NavButton.qml).
    pub icon_name: &'static str,
    pub section: Section,
    pub hide_on_compact: bool,
}

impl NavEntry {
    pub fn label(&self) -> String {
        tr(self.label_key, &[])
    }
}

/// The exact ordered rail model from Main.qml.
pub const NAV_ENTRIES: &[NavEntry] = &[
    NavEntry {
        page: Page::Dashboard,
        label_key: "\u{041F}\u{0430}\u{043D}\u{0435}\u{043B}\u{044C}",
        glyph: "\u{E80F}",
        icon_name: "",
        section: Section::Top,
        hide_on_compact: false,
    },
    NavEntry {
        page: Page::Nodes,
        label_key: "\u{0421}\u{0435}\u{0440}\u{0432}\u{0435}\u{0440}\u{044B}",
        glyph: "\u{EC05}",
        icon_name: "servers",
        section: Section::Top,
        hide_on_compact: false,
    },
    NavEntry {
        page: Page::Routing,
        label_key: "\u{041C}\u{0430}\u{0440}\u{0448}\u{0440}\u{0443}\u{0442}\u{044B}",
        glyph: "\u{E774}",
        icon_name: "routing",
        section: Section::Top,
        hide_on_compact: false,
    },
    NavEntry {
        page: Page::Zapret,
        label_key: "Zapret",
        glyph: "\u{E945}",
        icon_name: "zapret",
        section: Section::Top,
        hide_on_compact: false,
    },
    NavEntry {
        page: Page::History,
        label_key: "\u{0418}\u{0441}\u{0442}\u{043E}\u{0440}\u{0438}\u{044F}",
        glyph: "\u{E81C}",
        icon_name: "history",
        section: Section::Top,
        hide_on_compact: true,
    },
    NavEntry {
        page: Page::About,
        label_key: "\u{041E} \u{043F}\u{0440}\u{043E}\u{0435}\u{043A}\u{0442}\u{0435}",
        glyph: "\u{E946}",
        icon_name: "",
        section: Section::Bottom,
        hide_on_compact: false,
    },
    NavEntry {
        page: Page::Updates,
        label_key: "\u{041E}\u{0431}\u{043D}\u{043E}\u{0432}\u{043B}\u{0435}\u{043D}\u{0438}\u{044F}",
        glyph: "\u{E777}",
        icon_name: "",
        section: Section::Bottom,
        hide_on_compact: false,
    },
    NavEntry {
        page: Page::Logs,
        label_key: "\u{041B}\u{043E}\u{0433}\u{0438}",
        glyph: "\u{E8FD}",
        icon_name: "logs",
        section: Section::Bottom,
        hide_on_compact: true,
    },
    NavEntry {
        page: Page::Settings,
        label_key: "\u{041D}\u{0430}\u{0441}\u{0442}\u{0440}\u{043E}\u{0439}\u{043A}\u{0438}",
        glyph: "\u{E713}",
        icon_name: "",
        section: Section::Bottom,
        hide_on_compact: false,
    },
];

/// Page switching state (Main.qml `currentIndex` / `navExpanded`).
#[derive(Clone, Copy, Debug)]
pub struct NavigationState {
    pub current: Page,
    pub nav_expanded: bool,
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            current: Page::Dashboard,
            nav_expanded: false,
        }
    }
}

impl NavigationState {
    pub fn rail_collapsed(&self) -> bool {
        !self.nav_expanded
    }

    /// Main.qml Connections.onSettingsChanged: a freshly enabled compact mode
    /// bounces the user off pages the compact rail cannot reach.
    pub fn on_compact_mode_enabled(&mut self) {
        if matches!(self.current, Page::Configs | Page::Logs | Page::History) {
            self.current = Page::Dashboard;
        }
    }

    /// Bottom-section selection rule from Main.qml: the Settings entry also
    /// highlights while the Configs page is active.
    pub fn entry_selected(&self, entry: &NavEntry) -> bool {
        self.current.index() == entry.page.index()
            || (entry.page == Page::Settings && self.current == Page::Configs)
    }

    /// The compact rail can only reach pages without `hideOnCompact`.
    pub fn entry_visible(&self, entry: &NavEntry, compact_mode: bool) -> bool {
        !(compact_mode && entry.hide_on_compact)
    }
}

/// Outline path table from NavButton.qml for the entries that render an SVG
/// outline instead of a glyph.
pub fn outline_path(icon_name: &str) -> Option<&'static str> {
    match icon_name {
        "dashboard" => Some("M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z"),
        "servers" => Some("M4 3h16v7H4z M4 14h16v7H4z M7 6h.01 M7 17h.01 M11 6h6 M11 17h6"),
        "routing" => Some("M3 7h14 M14 4l3 3-3 3 M21 17H7 M10 14l-3 3 3 3"),
        "configs" => Some("M6 3h8l4 4v14H6z M14 3v5h5 M10 11l-2 3 2 3 M14 11l2 3-2 3"),
        "zapret" => Some("M12 2l8 3v6c0 5-3.5 9-8 11-4.5-2-8-6-8-11V5z M8.5 12l2.5 2.5 4.5-5"),
        "logs" => Some("M6 3h9l4 4v14H6z M14 3v5h5 M9 12h7 M9 15h7 M9 18h4"),
        "history" => Some("M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20 M12 6v6l4 2"),
        _ => None,
    }
}

/// Draw one NavButton inside the rail and report whether it was clicked.
/// Mirrors NavButton.qml geometry: 40px-tall rows, 6px inset, 38px compact hit
/// area, accent pill when selected, 8px red badge for the updates entry.
pub fn draw_nav_button(
    ui: &mut Ui,
    theme: &Theme,
    entry: &NavEntry,
    selected: bool,
    badge: bool,
    compact: bool,
) -> bool {
    let row_height = (40.0 * theme.font_scale) as f32;
    let row_width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(row_width, row_height), Sense::click());
    let hovered = response.hovered();

    let bg_rect = Rect::from_min_size(
        rect.min + Vec2::new(6.0, 0.0),
        Vec2::new(
            if compact {
                38.0
            } else {
                (row_width - 12.0).max(38.0)
            },
            row_height,
        ),
    );
    let fill = if selected {
        theme.accent_soft
    } else if hovered {
        theme.nav_hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(bg_rect, theme.radius_small as f32, fill);

    if selected {
        let pill_height = row_height * 0.5;
        let pill_rect = Rect::from_min_size(
            egui::pos2(bg_rect.left() + 3.0, bg_rect.center().y - pill_height / 2.0),
            Vec2::new(3.0, pill_height),
        );
        ui.painter().rect_filled(pill_rect, 1.5, theme.accent);
    }

    let icon_color = if selected {
        theme.text
    } else {
        theme.text_muted
    };
    let icon_size = (16.0 * theme.font_scale) as f32;
    let icon_pos = egui::pos2(bg_rect.left() + 11.0, bg_rect.center().y - icon_size / 2.0);
    let icon_rect = Rect::from_min_size(icon_pos, Vec2::new(18.0, icon_size));
    ui.painter().text(
        icon_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        entry.glyph,
        egui::FontId::proportional(icon_size),
        icon_color,
    );

    if badge {
        // 8px circle, #E81123 fill with the flyout border, on the glyph corner.
        let center = egui::pos2(icon_rect.right() + 3.0, icon_rect.top() + 1.0);
        ui.painter()
            .circle_filled(center, 4.0, Color32::from_rgb(0xE8, 0x11, 0x23));
        ui.painter()
            .circle_stroke(center, 4.5, egui::Stroke::new(1.0, theme.flyout));
    }

    if !compact {
        let label_pos = egui::pos2(bg_rect.left() + 41.0, rect.center().y);
        ui.painter().text(
            label_pos,
            egui::Align2::LEFT_CENTER,
            entry.label(),
            egui::FontId::proportional(theme.font_normal as f32),
            icon_color,
        );
    }

    response.clicked()
}

/// Whether the updates nav entry shows the red badge (index 7 +
/// App.updatesAvailable in Main.qml).
pub fn updates_badge(entry: &NavEntry, updates_available: bool) -> bool {
    entry.page == Page::Updates && updates_available
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rail_order_matches_main_qml() {
        let pages: Vec<Page> = NAV_ENTRIES.iter().map(|e| e.page).collect();
        assert_eq!(
            pages,
            vec![
                Page::Dashboard,
                Page::Nodes,
                Page::Routing,
                Page::Zapret,
                Page::History,
                Page::About,
                Page::Updates,
                Page::Logs,
                Page::Settings,
            ]
        );
        assert_eq!(NAV_ENTRIES[0].section, Section::Top);
        assert_eq!(NAV_ENTRIES[4].section, Section::Top);
        assert_eq!(NAV_ENTRIES[5].section, Section::Bottom);
    }

    #[test]
    fn glyphs_are_kept_byte_identical() {
        assert_eq!(NAV_ENTRIES[0].glyph, "\u{E80F}");
        assert_eq!(NAV_ENTRIES[1].glyph, "\u{EC05}");
        assert_eq!(NAV_ENTRIES[2].glyph, "\u{E774}");
        assert_eq!(NAV_ENTRIES[3].glyph, "\u{E945}");
        assert_eq!(NAV_ENTRIES[4].glyph, "\u{E81C}");
        assert_eq!(NAV_ENTRIES[5].glyph, "\u{E946}");
        assert_eq!(NAV_ENTRIES[6].glyph, "\u{E777}");
        assert_eq!(NAV_ENTRIES[7].glyph, "\u{E8FD}");
        assert_eq!(NAV_ENTRIES[8].glyph, "\u{E713}");
    }

    #[test]
    fn hide_on_compact_flags_match_the_model() {
        let hidden: Vec<Page> = NAV_ENTRIES
            .iter()
            .filter(|e| e.hide_on_compact)
            .map(|e| e.page)
            .collect();
        assert_eq!(hidden, vec![Page::History, Page::Logs]);
        // The Zapret label bypasses I18n.t in Main.qml.
        assert_eq!(NAV_ENTRIES[3].label_key, "Zapret");
    }

    #[test]
    fn compact_mode_bounces_off_unreachable_pages() {
        let mut state = NavigationState::default();
        state.current = Page::Configs;
        state.on_compact_mode_enabled();
        assert_eq!(state.current, Page::Dashboard);
        state.current = Page::Logs;
        state.on_compact_mode_enabled();
        assert_eq!(state.current, Page::Dashboard);
        state.current = Page::History;
        state.on_compact_mode_enabled();
        assert_eq!(state.current, Page::Dashboard);
        state.current = Page::Nodes;
        state.on_compact_mode_enabled();
        assert_eq!(state.current, Page::Nodes);
    }

    #[test]
    fn settings_entry_also_highlights_for_configs() {
        let mut state = NavigationState::default();
        let settings = &NAV_ENTRIES[8];
        assert!(!state.entry_selected(settings));
        state.current = Page::Configs;
        assert!(state.entry_selected(settings));
        assert!(!state.entry_selected(&NAV_ENTRIES[0]));
    }

    #[test]
    fn updates_badge_only_applies_to_the_updates_entry() {
        let updates = &NAV_ENTRIES[6];
        assert!(updates_badge(updates, true));
        assert!(!updates_badge(updates, false));
        assert!(!updates_badge(&NAV_ENTRIES[0], true));
    }

    #[test]
    fn outline_paths_cover_the_named_icons() {
        assert!(outline_path("servers").is_some());
        assert!(outline_path("routing").is_some());
        assert!(outline_path("zapret").is_some());
        assert!(outline_path("logs").is_some());
        assert!(outline_path("history").is_some());
        assert!(outline_path("").is_none());
        assert!(outline_path("unknown").is_none());
    }
}
