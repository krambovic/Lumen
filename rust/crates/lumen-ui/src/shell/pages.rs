//! Page dispatch for the `LazyPageLoader` stack in `Main.qml`.

use egui::{Context, Ui};

use lumen_core::i18n::tr;

use crate::view_models::{AppBridge, app_bridge::UiEvent};

use super::navigation::Page;
use super::theme::Theme;

mod code_editor;
mod configs;
mod diagnostics_export;
mod history;
mod logs;
mod routing;
mod updates;
mod zapret;

#[derive(Default)]
pub(super) struct PageState {
    configs: configs::ConfigsPage,
    history: history::HistoryPage,
    logs: logs::LogsPage,
    routing: routing::RoutingPage,
    updates: updates::UpdatesPage,
    zapret: zapret::ZapretPage,
}

impl PageState {
    pub(super) fn on_event(&mut self, event: &UiEvent) {
        self.updates.on_event(event);
    }
}

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
pub(super) fn render_page(
    page: Page,
    ui: &mut Ui,
    page_ctx: &mut PageContext,
    state: &mut PageState,
) {
    if page != Page::Logs {
        state.logs.on_hidden();
    }
    if page != Page::Zapret {
        state.zapret.on_hidden();
    }
    match page {
        Page::Dashboard => placeholder(ui, page_ctx.theme, page),
        Page::Nodes => placeholder(ui, page_ctx.theme, page),
        Page::Routing => state.routing.render(ui, page_ctx),
        Page::Configs => state.configs.render(ui, page_ctx),
        Page::Zapret => state.zapret.render(ui, page_ctx),
        Page::Logs => state.logs.render(ui, page_ctx),
        Page::History => state.history.render(ui, page_ctx),
        Page::Updates => state.updates.render(ui, page_ctx),
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
