//! Page dispatch skeleton (the `LazyPageLoader` stack inside Main.qml's
//! `pageStack`).
//!
//! # Extension point for page agents
//!
//! `render_page` is the single seam the application shell uses to paint the
//! active page. Two more agents port the pages from `qml_app/qml/*.qml` into
//! this module (`pages/dashboard.rs`, `pages/nodes.rs`, ...); every arm below
//! becomes `super::nodes::render(ui, page_ctx)` once that lands. The pages
//! receive a [`PageContext`] with the resolved [`Theme`], the view-model
//! bridge and shell hooks — nothing else should reach the window frame.
//!
//! Until the page modules exist each arm renders a themed placeholder panel
//! carrying the localized page title, so navigation state remains testable.

use egui::{Context, Ui};

use lumen_core::i18n::tr;

use crate::view_models::{AppBridge, app_bridge::UiEvent};

use super::navigation::Page;
use super::theme::Theme;

/// Everything a page implementation may touch, re-exported to the future
/// `pages/*` modules.
pub struct PageContext<'a> {
    pub ctx: &'a Context,
    pub theme: &'a Theme,
    pub bridge: &'a mut AppBridge,
    pub events: &'a [UiEvent],
    /// Shell-owned hooks: page agents request navigation changes through
    /// these (Main.qml `onOpenConfigsRequested`, config-drop queueing, ...).
    pub shell: &'a mut ShellHooks,
}

#[derive(Default)]
pub struct ShellHooks {
    /// `win.currentIndex = 3` from `SettingsPage.onOpenConfigsRequested`.
    pub open_configs_requested: bool,
}

/// Render the active page into the content window.
pub fn render_page(page: Page, ui: &mut Ui, page_ctx: &mut PageContext) {
    match page {
        // EXTENSION POINT: replace each arm with the real page module when it
        // lands (STATUS.md lists the pages as a separate porting assignment).
        Page::Dashboard => placeholder(ui, page_ctx.theme, page),
        Page::Nodes => placeholder(ui, page_ctx.theme, page),
        Page::Routing => placeholder(ui, page_ctx.theme, page),
        Page::Configs => placeholder(ui, page_ctx.theme, page),
        Page::Zapret => placeholder(ui, page_ctx.theme, page),
        Page::Logs => placeholder(ui, page_ctx.theme, page),
        Page::History => placeholder(ui, page_ctx.theme, page),
        Page::Updates => placeholder(ui, page_ctx.theme, page),
        Page::Settings => placeholder(ui, page_ctx.theme, page),
        Page::About => placeholder(ui, page_ctx.theme, page),
    }
}

/// The themed stand-in panel for pages that have not been ported yet.
fn placeholder(ui: &mut Ui, theme: &Theme, page: Page) {
    ui.vertical_centered(|ui| {
        ui.add_space(theme.spacing_large as f32 * 2.0);
        ui.label(
            egui::RichText::new(tr(page.title(), &[]))
                .size(theme.font_title as f32)
                .color(theme.text),
        );
        ui.add_space(theme.spacing as f32);
        ui.label(
            egui::RichText::new("\u{E8FD}")
                .size(theme.font_hero as f32)
                .color(theme.text_faint),
        );
    });
}
