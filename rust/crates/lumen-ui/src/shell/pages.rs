//! Page dispatch for the `LazyPageLoader` stack in `Main.qml`.

use egui::{Context, Ui};

use crate::view_models::{AppBridge, app_bridge::UiEvent};

use super::navigation::Page;
use super::theme::Theme;

pub(super) mod about;
mod code_editor;
mod configs;
mod dashboard;
mod diagnostics_export;
pub(crate) mod flag;
pub(crate) mod fluent_button;
mod history;
mod logs;
mod node_editor;
mod node_qr;
mod nodes;
mod routing;
mod search_field;
pub(crate) mod settings;
mod text_edit_menu;
mod updates;
mod zapret;

#[derive(Default)]
pub(super) struct PageState {
    about: about::AboutPage,
    configs: configs::ConfigsPage,
    dashboard: dashboard::DashboardPage,
    node_qr: node_qr::NodeQr,
    nodes: nodes::NodesPage,
    history: history::HistoryPage,
    logs: logs::LogsPage,
    routing: routing::RoutingPage,
    settings: settings::SettingsPage,
    updates: updates::UpdatesPage,
    zapret: zapret::ZapretPage,
}

impl PageState {
    pub(super) fn on_event(&mut self, event: &UiEvent) {
        self.updates.on_event(event);
        self.node_qr.on_event(event);
        self.nodes.on_event(event);
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
    let qr_dialog_open = state.node_qr.is_open();
    if page != Page::Logs {
        state.logs.on_hidden();
    }
    if page != Page::Zapret {
        state.zapret.on_hidden();
    }
    state
        .dashboard
        .observe(page_ctx.bridge, page == Page::Dashboard);
    render_page_body(page, ui, page_ctx, state, qr_dialog_open);
    state.node_qr.render(page_ctx.ctx, page_ctx.theme);
}

/// A page that is still fading out after a page change. `LazyPageLoader`
/// keeps a page laid out while its opacity is above zero, so the old and the
/// new page cross-fade.
pub(super) fn render_leaving_page(
    page: Page,
    ui: &mut Ui,
    page_ctx: &mut PageContext,
    state: &mut PageState,
) {
    let qr_dialog_open = state.node_qr.is_open();
    render_page_body(page, ui, page_ctx, state, qr_dialog_open);
}

fn render_page_body(
    page: Page,
    ui: &mut Ui,
    page_ctx: &mut PageContext,
    state: &mut PageState,
    qr_dialog_open: bool,
) {
    match page {
        Page::Dashboard => state.dashboard.render(ui, page_ctx),
        Page::Nodes => state.nodes.render(ui, page_ctx, qr_dialog_open),
        Page::Routing => state.routing.render(ui, page_ctx),
        Page::Configs => state.configs.render(ui, page_ctx),
        Page::Zapret => state.zapret.render(ui, page_ctx),
        Page::Logs => state.logs.render(ui, page_ctx),
        Page::History => state.history.render(ui, page_ctx),
        Page::Updates => state.updates.render(ui, page_ctx),
        Page::Settings => state.settings.render(ui, page_ctx),
        Page::About => state.about.render(ui, page_ctx),
    }
}
