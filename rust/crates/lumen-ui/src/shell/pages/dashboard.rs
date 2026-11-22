//! Connection controls, server shortcuts and live metrics from `DashboardPage.qml`.

use std::collections::{HashSet, VecDeque};

use egui::{
    Align, Color32, FontId, Id, Key, Layout, Modifiers, Rect, ScrollArea, Sense,
    Ui, pos2, vec2,
};
use lumen_core::i18n::tr;

use super::{PageContext, flag, node_editor::NodeEditor, search_field};
use crate::shell::motion;
use crate::shell::theme::{Theme, semibold};
use crate::shell::widgets::{self, ButtonKind, FluentButton};
use crate::view_models::AppBridge;

const GAP: f32 = 16.0;
const SERVER_WIDTH: f32 = 420.0;
/// Latency / session cards: 12 px padding around title, value and chart.
const STAT_CARD_HEIGHT: f32 = 96.0;
const ROUTING_CARD_HEIGHT: f32 = 60.0;
/// 44 px server rows, 2 px apart.
const ROW_PITCH: f32 = 46.0;

#[derive(Default)]
pub(super) struct DashboardPage {
    query: String,
    quick_hint: bool,
    selection: HashSet<String>,
    latency: VecDeque<i64>,
    peak: f64,
    app_peak: f64,
    mode_help: bool,
    ring: widgets::StatusRing,
    editor: NodeEditor,
    initialized: bool,
}

impl DashboardPage {
    pub(super) fn observe(&mut self, bridge: &AppBridge, active: bool) {
        if !active {
            self.selection.clear();
        }
        if !bridge.connected() {
            self.latency.clear();
            self.peak = 0.0;
            self.app_peak = 0.0;
        } else if bridge.changes().metrics {
            let metrics = bridge.metrics();
            if metrics.traffic_available {
                self.peak = self.peak.max(metrics.up_bps).max(metrics.down_bps);
            }
            if metrics.latency_ms > 0 {
                if self.latency.len() == 16 {
                    self.latency.pop_front();
                }
                self.latency.push_back(metrics.latency_ms);
            }
            for row in bridge.process_model.rows() {
                self.app_peak = self.app_peak.max(row.total);
            }
        }
    }

    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        let bridge = &mut *page.bridge;
        if !self.initialized {
            bridge.set_process_traffic_sort("total", false);
            self.initialized = true;
        }
        if !egui::Popup::is_any_open(page.ctx)
            && !self.mode_help
            && !self.editor.is_open()
            && ui.input(|input| input.pointer.primary_pressed() && !input.modifiers.ctrl)
        {
            self.selection.clear();
        }
        let page_width = ui.available_width();
        let content_height = ui.available_height().max(620.0);
        ScrollArea::vertical()
            .id_salt("dashboard")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                let top = ui.cursor().top();
                self.connection(ui, theme, bridge);
                let body_height = (content_height - (ui.cursor().top() - top)).max(0.0);
                if page_width >= 960.0 {
                    ui.horizontal_top(|ui| {
                        let stats_width = (ui.available_width() - SERVER_WIDTH - GAP).max(0.0);
                        column(ui, SERVER_WIDTH, |ui| {
                            self.servers(ui, theme, bridge, body_height);
                        });
                        column(ui, stats_width, |ui| {
                            self.stats(ui, theme, bridge, Some(body_height), true);
                        });
                    });
                } else {
                    self.servers(ui, theme, bridge, 420.0);
                    self.stats(ui, theme, bridge, None, page_width >= 720.0);
                }
            });
        if self.mode_help {
            self.mode_help = mode_help(page.ctx, theme);
        }
        self.editor.render(page.ctx, theme, bridge);
    }

    fn connection(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let show_rates = ui.available_width() >= 760.0;
        widgets::card_frame(theme, 20.0).show(ui, |ui| {
            let connected = bridge.connected();
            let busy = bridge.transition_busy();
            let state = match (busy, bridge.transition_target_connected(), connected) {
                (true, true, _) => "Подключение…",
                (true, false, _) => "Отключение…",
                (false, _, true) => "Подключено",
                _ => "Ожидание",
            };
            let mode = if bridge.tun_mode() {
                "tun"
            } else if bridge.proxy_enabled() {
                "system"
            } else {
                "local"
            };
            let mode_label = match mode {
                "tun" => "VPN (TUN)",
                "system" => "Системный прокси",
                _ => "Локальный прокси",
            };
            let name = if bridge.selected_node_name().is_empty() {
                tr("Сервер не выбран", &[])
            } else {
                bridge.selected_node_name().to_string()
            };
            let caption = [
                bridge.selected_node_protocol(),
                bridge.selected_node_source(),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
            let metrics = bridge.metrics();
            let small_font = FontId::proportional(theme.font_small as f32);

            // RowLayout { ColumnLayout { RowLayout { ring 52 | text block } +
            // 12 + buttons (32) } | rates }. The top row is as tall as the
            // ring or the text block, whichever is taller: 52 px while the
            // caption line is hidden, more once a server adds it.
            let row_height = |font: FontId| ui.fonts_mut(|fonts| fonts.row_height(&font));
            let status_height = row_height(small_font.clone());
            let title_height = row_height(semibold(theme.font_title as f32 + 2.0)).max(24.0);
            let caption_height = if caption.is_empty() { 0.0 } else { 2.0 + status_height };
            let text_block = status_height + 2.0 + title_height + caption_height;
            let top_height = text_block.max(52.0).ceil();
            let width = ui.available_width();
            ui.set_min_width(width);
            let (rect, _) = ui.allocate_exact_size(vec2(width, top_height + 12.0 + 32.0), Sense::hover());
            let painter = ui.painter().clone();

            let mut left_width = width;
            if show_rates {
                let columns = [
                    ("↓", "Загрузка", metrics.down_bps),
                    ("↑", "Выгрузка", metrics.up_bps),
                ]
                .map(|(arrow, label, rate)| {
                    let rate = if !connected {
                        "0 B/s".to_string()
                    } else if metrics.traffic_available {
                        speed(rate)
                    } else {
                        "—".to_string()
                    };
                    let (value, unit) = rate.split_once(' ').unwrap_or((rate.as_str(), ""));
                    (
                        widgets::galley(
                            ui,
                            &format!("{arrow} {}", tr(label, &[])),
                            small_font.clone(),
                            theme.text_muted,
                        ),
                        widgets::BigValue::new(ui, theme, value, unit, theme.font_title as f32),
                    )
                });
                let column_width =
                    |column: &(std::sync::Arc<egui::Galley>, widgets::BigValue)| {
                        column.0.size().x.max(column.1.size().x)
                    };
                let total = column_width(&columns[0]) + 36.0 + column_width(&columns[1]);
                let mut x = rect.right() - 8.0 - total;
                for column in &columns {
                    let height = column.0.size().y + 2.0 + column.1.size().y;
                    let y = rect.center().y - height / 2.0;
                    painter.galley(pos2(x, y), column.0.clone(), theme.text_muted);
                    column.1.paint(&painter, pos2(x, y + column.0.size().y + 2.0));
                    x += column_width(column) + 36.0;
                }
                left_width = (width - total - 8.0 - 24.0).max(0.0);
            }

            self.ring.show(
                ui,
                theme,
                Rect::from_min_size(rect.min + vec2(0.0, ((top_height - 52.0) / 2.0).round()), vec2(52.0, 52.0)),
                connected,
                busy,
                bridge.transition_target_connected(),
            );

            let text_left = rect.left() + 66.0;
            let text_width = (left_width - 66.0).max(0.0);
            let status = widgets::elided(
                ui,
                &format!("{} · {}", tr(state, &[]), tr(mode_label, &[])),
                small_font.clone(),
                if connected {
                    theme.success
                } else {
                    theme.text_muted
                },
                text_width,
            );
            let country = bridge.selected_node_flag_country().to_string();
            let flag_width = if country.is_empty() { 0.0 } else { 42.0 };
            let title = widgets::elided(
                ui,
                &name,
                semibold(theme.font_title as f32 + 2.0),
                theme.text,
                text_width - flag_width,
            );
            let caption = (!caption.is_empty())
                .then(|| widgets::elided(ui, &caption, small_font, theme.text_muted, text_width));
            let title_height = title.size().y.max(24.0);
            let block = status.size().y
                + 2.0
                + title_height
                + caption.as_ref().map_or(0.0, |galley| 2.0 + galley.size().y);
            let mut y = rect.top() + (top_height - block) / 2.0;
            painter.galley(pos2(text_left, y), status.clone(), theme.text_muted);
            y += status.size().y + 2.0;
            if flag_width > 0.0 {
                let flag_rect = Rect::from_min_size(
                    pos2(text_left, y + (title_height - 24.0) / 2.0),
                    vec2(32.0, 24.0),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(flag_rect), |ui| {
                    flag::show(ui, &country, vec2(32.0, 24.0), flag::FlagBorder::UnderSprite { radius: 3 });
                });
            }
            painter.galley(
                pos2(text_left + flag_width, y + (title_height - title.size().y) / 2.0),
                title,
                theme.text,
            );
            y += title_height + 2.0;
            if let Some(caption) = caption {
                painter.galley(pos2(text_left, y), caption, theme.text_muted);
            }

            let row = Rect::from_min_size(
                pos2(rect.left(), rect.top() + top_height + 12.0),
                vec2(left_width.max(280.0).min(width), 32.0),
            );
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(row)
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (kind, glyph, key) = if connected {
                        (ButtonKind::Ghost, "\u{f8ae}", "Отключить")
                    } else {
                        (ButtonKind::Accent, "\u{e768}", "Подключить")
                    };
                    if FluentButton::new(kind, glyph, &tr(key, &[]))
                        .width(180.0)
                        .enabled(!busy)
                        .show(ui, theme)
                        .clicked()
                    {
                        bridge.toggle_connection();
                    }
                    if let Some(key) = mode_switch(ui, theme, mode, busy) {
                        match key {
                            "tun" => {
                                bridge.set_tun(true);
                            }
                            "system" => {
                                bridge.set_proxy(true);
                            }
                            _ => {
                                if bridge.tun_mode() {
                                    bridge.set_tun(false);
                                }
                                if bridge.proxy_enabled() {
                                    bridge.set_proxy(false);
                                }
                            }
                        }
                    }
                    if FluentButton::icon("\u{e946}", &tr("Справка", &[]))
                        .show(ui, theme)
                        .clicked()
                    {
                        self.mode_help = true;
                    }
                },
            );
        });
    }

    fn servers(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, height: f32) {
        widgets::card_frame(theme, 12.0).show(ui, |ui| {
            let inner_height = height - 24.0;
            let top = ui.cursor().top();
            let width = ui.available_width();
            ui.set_min_width(width);
            ui.set_min_height(inner_height);
            ui.spacing_mut().item_spacing = vec2(0.0, 10.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                widgets::section_title(ui, theme, &tr("Серверы", &[]));
            });
            let mut groups = bridge.group_options();
            for subscription in bridge.subscriptions() {
                let group = subscription["group"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .or_else(|| subscription["name"].as_str())
                    .unwrap_or("Default");
                if !groups.iter().any(|name| name == group) {
                    groups.push(group.into());
                }
            }
            let current = bridge.node_filter_group();
            // Group tabs: the underline marks the active group (Fluent pivot).
            ScrollArea::horizontal()
                .id_salt("dashboard-groups")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (group, label) in std::iter::once(("", tr("Все", &[])))
                            .chain(groups.iter().map(|name| (name.as_str(), name.clone())))
                        {
                            let active = group == current;
                            let size = theme.font_normal as f32;
                            let tab_font = if active {
                                semibold(size)
                            } else {
                                FontId::proportional(size)
                            };
                            let text_width =
                                widgets::galley(ui, &label, tab_font, theme.text).size().x;
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(text_width + 20.0, 34.0), Sense::hover());
                            let id = Id::unique(("dashboard-group-tab", group));
                            let tab = widgets::hit(ui, rect, id);
                            let (font, color) = if active {
                                (semibold(size), theme.text)
                            } else if tab.hovered() {
                                (FontId::proportional(size), theme.text)
                            } else {
                                (FontId::proportional(size), theme.text_muted)
                            };
                            let text = widgets::galley(ui, &label, font, color);
                            ui.painter().galley(
                                rect.center() - text.size() / 2.0 - vec2(0.0, 2.0),
                                text,
                                color,
                            );
                            let underline = motion::follow(
                                ui.ctx(),
                                theme,
                                id.with("underline"),
                                if active { (text_width - 4.0).max(16.0) } else { 0.0 },
                                theme.anim_fast as f32 * 2.0,
                                motion::ease_out_cubic,
                            );
                            if underline > 0.5 {
                                ui.painter().rect_filled(
                                    Rect::from_center_size(
                                        pos2(rect.center().x, rect.bottom() - 3.5),
                                        vec2(underline, 3.0),
                                    ),
                                    1.5,
                                    theme.accent,
                                );
                            }
                            if tab.clicked() {
                                self.selection.clear();
                                bridge.set_node_filter(group, "");
                            }
                        }
                    });
                });
            // `Layout.topMargin: -10`: the divider sits right under the tabs.
            ui.add_space(-5.0);
            let (divider, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
            ui.painter().rect_filled(divider, 0.0, theme.divider);
            ui.add_space(4.0);
            let (search, cleared) = search_field::show(
                ui,
                theme,
                &mut self.query,
                &tr("Поиск по имени, протоколу, стране", &[]),
                true,
            );
            if search.changed() || cleared {
                self.selection.clear();
                bridge.set_dashboard_node_filter(&self.query);
            }
            let ids = bridge.dashboard_node_ids();
            if !search.has_focus()
                && !self.mode_help
                && !self.editor.is_open()
                && !egui::Popup::is_any_open(ui.ctx())
            {
                if ui.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::A)) {
                    self.selection = ids.iter().cloned().collect();
                }
                if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    self.selection.clear();
                }
            }
            let list_height = (inner_height - (ui.cursor().top() - top)).max(0.0);
            let hint = self.quick_hint.then(|| {
                ui.painter().layout(
                    tr(
                        "Нет серверов с измеренным пингом — проверьте пинг на странице «Серверы»",
                        &[],
                    ),
                    FontId::proportional(theme.font_small as f32),
                    theme.warning,
                    (width - 20.0).max(0.0),
                )
            });
            // ListView: the quick-connect header scrolls with the 44 px rows (2 px apart).
            let header_height = 44.0 + hint.as_ref().map_or(0.0, |hint| hint.size().y + 6.0) + 2.0;
            let rows_height = if ids.is_empty() {
                0.0
            } else {
                ids.len() as f32 * ROW_PITCH - 2.0
            };
            let list = ScrollArea::vertical()
                .id_salt("dashboard-node-list")
                .max_height(list_height)
                .auto_shrink([false, false])
                .show_viewport(ui, |ui, viewport| {
                    ui.set_height(header_height + rows_height);
                    let origin = ui.min_rect().min;
                    let quick = Rect::from_min_size(origin, vec2(width - 10.0, 44.0));
                    let quick_response = widgets::hit(ui, quick, Id::unique("dashboard-quick-connect"));
                    if quick_response.hovered() {
                        ui.painter()
                            .rect_filled(quick, theme.radius_small as f32, theme.card_hover);
                    }
                    ui.painter().text(
                        pos2(quick.left() + 21.0, quick.center().y),
                        egui::Align2::CENTER_CENTER,
                        "\u{e945}",
                        FontId::proportional(16.0),
                        theme.accent,
                    );
                    two_lines(
                        ui,
                        theme,
                        pos2(quick.left() + 44.0, quick.center().y),
                        quick.width() - 54.0,
                        &tr("Быстрое подключение", &[]),
                        &tr("Лучший пинг в группе", &[]),
                        theme.text,
                    );
                    if let Some(hint) = &hint {
                        ui.painter().galley(
                            pos2(origin.x + 10.0, quick.bottom() + 4.0),
                            hint.clone(),
                            theme.warning,
                        );
                    }
                    if !bridge.transition_busy() && quick_response.clicked() {
                        let id = bridge.quick_connect_node_id();
                        self.quick_hint = id.is_empty();
                        if !self.quick_hint {
                            bridge.select_node(&id);
                            if !bridge.connected() && !bridge.transition_busy() {
                                bridge.toggle_connection();
                            }
                        }
                    }
                    let first = ((viewport.top() - header_height) / ROW_PITCH).floor().max(0.0) as usize;
                    let last = (((viewport.bottom() - header_height) / ROW_PITCH).ceil().max(0.0)
                        as usize)
                        .min(ids.len());
                    for (index, id) in ids.iter().enumerate().take(last).skip(first) {
                        let row = Rect::from_min_size(
                            origin + vec2(0.0, header_height + index as f32 * ROW_PITCH),
                            vec2(width, 44.0),
                        );
                        self.server_row(ui, theme, bridge, id, row);
                    }
                });
            if ids.is_empty() {
                ui.painter().text(
                    list.inner_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    tr("Серверов нет — импортируйте ссылку или подписку", &[]),
                    FontId::proportional(theme.font_small as f32),
                    theme.text_faint,
                );
            }
        });
    }

    fn server_row(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        id: &str,
        rect: Rect,
    ) {
        let Ok(index) = usize::try_from(bridge.node_model.index_of_id(id)) else {
            return;
        };
        let model = &bridge.node_model;
        let active = model.selected_at(index);
        let picked = self.selection.contains(id);
        let ping = model.ping_at(index);
        let supported = model.runtime_supported_at(index);
        let name = model.name_at(index);
        let sub = [model.protocol_at(index), model.group_at(index)]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let country = model.country_at(index);
        let row_id = Id::unique(("dashboard-server-row", id));
        let row = widgets::hit(ui, rect, row_id);
        let menu_open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&row));
        let dim = if supported { 1.0 } else { 0.55 };

        let body = Rect::from_min_max(rect.min, rect.max - vec2(10.0, 0.0));
        let fill = if active {
            theme.accent_soft
        } else if picked {
            theme.accent.gamma_multiply(0.12)
        } else if row.hovered() || menu_open {
            theme.card_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(body, theme.radius_small as f32, fill.gamma_multiply(dim));
        if active {
            ui.painter().rect_filled(
                Rect::from_min_size(pos2(body.left(), body.center().y - 8.0), vec2(3.0, 16.0)),
                2.0,
                theme.accent.gamma_multiply(dim),
            );
        }
        let flag_rect = Rect::from_min_size(
            pos2(body.left() + 10.0, body.center().y - 8.0),
            vec2(22.0, 16.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(flag_rect), |ui| {
            ui.multiply_opacity(dim);
            flag::show(ui, &country, vec2(22.0, 16.0), flag::FlagBorder::Over { radius: 2 });
        });
        let latency = if ping < 0 {
            "—".to_string()
        } else {
            format!("{ping} ms")
        };
        let latency = widgets::galley(
            ui,
            &latency,
            FontId::proportional(theme.font_small as f32),
            theme.ping_color((ping >= 0).then_some(ping)).gamma_multiply(dim),
        );
        let latency_left = body.right() - 10.0 - latency.size().x;
        ui.painter().galley(
            pos2(latency_left, body.center().y - latency.size().y / 2.0),
            latency,
            theme.text_muted,
        );
        let text_left = body.left() + 44.0;
        two_lines(
            ui,
            theme,
            pos2(text_left, body.center().y),
            latency_left - 12.0 - text_left,
            &name,
            &sub,
            theme.text.gamma_multiply(dim),
        );

        if row.clicked() {
            if ui.input(|input| input.modifiers.ctrl) {
                if !self.selection.remove(id) {
                    self.selection.insert(id.into());
                }
            } else {
                self.selection.clear();
                bridge.select_node(id);
            }
        }
        crate::shell::fluent_menu::context_menu(&row, theme, 270.0, |ui| {
            self.server_menu(ui, theme, bridge, id);
        });
    }

    /// `ctxMenu` in `DashboardPage.qml` (270 px, no icons).
    fn server_menu(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, id: &str) {
        use crate::shell::fluent_menu::{Item, separator};
        let multi = self.selection.len() > 1 && self.selection.contains(id);
        let targets = if multi {
            self.selection.clone()
        } else {
            HashSet::from([id.into()])
        };
        let item = |ui: &mut Ui, label: &str, enabled: bool| {
            Item::new("", label).enabled(enabled).show(ui, theme, false)
        };
        if item(ui, &tr("Установить как активный", &[]), !multi) {
            bridge.select_node(id);
        }
        separator(ui, theme);
        if item(ui, &tr("Редактировать", &[]), !multi) {
            self.editor.edit(bridge, id);
        }
        if item(ui, &tr("Копировать ссылку", &[]), !multi)
            && let Some(link) = bridge.copy_node_link(id)
        {
            ui.ctx().copy_text(link);
        }
        if item(ui, &tr("Экспорт в QR-код", &[]), !multi) {
            bridge.show_node_qr(id);
        }
        separator(ui, theme);
        for (label, action) in [
            ("Пинг — способ из настроек", 0),
            ("Тест TCP ping", 1),
            ("Тест HTTP GET", 2),
            ("Тест реальной задержки", 3),
        ] {
            if item(ui, &count_label(label, targets.len()), true) {
                match action {
                    0 => bridge.ping_nodes(Some(&targets)),
                    1 => bridge.tcping_nodes(Some(&targets)),
                    2 => bridge.http_get_nodes(Some(&targets)),
                    _ => bridge.real_delay_nodes(Some(&targets)),
                }
            }
        }
        separator(ui, theme);
        for (label, direction) in [("В начало списка", "top"), ("В конец списка", "bottom")] {
            if item(ui, &tr(label, &[]), !multi) {
                bridge.reorder_node(id, direction);
            }
        }
        separator(ui, theme);
        if item(ui, &count_label("Удалить", targets.len()), true) {
            bridge.delete_nodes(&targets.into_iter().collect::<Vec<_>>());
            self.selection.clear();
        }
    }

    fn stats(
        &self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        height: Option<f32>,
        side_by_side: bool,
    ) {
        let small_font = FontId::proportional(theme.font_small as f32);
        let latency = |ui: &mut Ui| {
            widgets::card_frame(theme, 12.0).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(STAT_CARD_HEIGHT - 24.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
                widgets::section_title(ui, theme, &tr("Задержка", &[]));
                let current = bridge.metrics().latency_ms;
                let value = if bridge.connected() && current >= 0 {
                    current.to_string()
                } else {
                    "--".into()
                };
                widgets::BigValue::new(ui, theme, &value, "ms", theme.font_title as f32).show(ui);
                latency_chart(ui, theme, &self.latency);
            });
        };
        let session = |ui: &mut Ui| {
            widgets::card_frame(theme, 12.0).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(STAT_CARD_HEIGHT - 24.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
                widgets::section_title(ui, theme, &tr("Сессия", &[]));
                let total = bytes(bridge.session_traffic_total() as f64);
                let (value, unit) = total.split_once(' ').unwrap_or((total.as_str(), ""));
                widgets::BigValue::new(ui, theme, value, unit, theme.font_title as f32).show(ui);
                let summary = widgets::elided(
                    ui,
                    &format!(
                        "↓ {} · ↑ {}  ·  {}{}",
                        bytes(bridge.session_download_total() as f64),
                        bytes(bridge.session_upload_total() as f64),
                        tr("Пик: ", &[]),
                        speed(self.peak)
                    ),
                    small_font.clone(),
                    theme.text_muted,
                    ui.available_width(),
                );
                let (rect, _) = ui.allocate_exact_size(summary.size(), Sense::hover());
                ui.painter().galley(rect.min, summary, theme.text_muted);
            });
        };
        if side_by_side {
            ui.horizontal_top(|ui| {
                let width = (ui.available_width() - GAP) / 2.0;
                column(ui, width, latency);
                column(ui, width, session);
            });
        } else {
            latency(ui);
            session(ui);
        }
        let rows = bridge.process_model.rows();
        let traffic_height = height.map_or(
            (rows.len().min(6) as f32 * 30.0 + 60.0).max(150.0),
            |height| (height - STAT_CARD_HEIGHT - ROUTING_CARD_HEIGHT - 2.0 * GAP).max(150.0),
        );
        widgets::card_frame(theme, 16.0).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(traffic_height - 32.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 10.0);
            let title = widgets::section_title(ui, theme, &tr("Трафик приложений", &[]));
            let body_height = (traffic_height - 32.0 - title.rect.height() - 10.0).max(0.0);
            if rows.is_empty() {
                // The empty ListView and the trailing filler share the free
                // height, which centres the hint.
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), body_height), Sense::hover());
                let hint = ui.painter().layout(
                    tr("Статистика появится, когда приложения начнут передавать данные", &[]),
                    small_font.clone(),
                    theme.text_faint,
                    rect.width(),
                );
                ui.painter().galley(
                    pos2(rect.left(), rect.center().y - hint.size().y / 2.0),
                    hint,
                    theme.text_faint,
                );
                return;
            }
            ui.spacing_mut().item_spacing.y = 4.0;
            ScrollArea::vertical()
                .id_salt("dashboard-process-traffic")
                .max_height(body_height)
                .auto_shrink([false, false])
                .show_rows(ui, 26.0, rows.len(), |ui, range| {
                    let font = FontId::proportional(theme.font_small as f32 + 1.0);
                    for row in &rows[range] {
                        let name = row.name.rsplit(['/', '\\']).next().unwrap_or(&row.name);
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
                        let right = rect.right() - 8.0;
                        let label = widgets::elided(ui, name, font.clone(), theme.text, 150.0);
                        ui.painter().galley(
                            pos2(rect.left(), rect.center().y - label.size().y / 2.0),
                            label,
                            theme.text,
                        );
                        let total = widgets::galley(ui, &bytes(row.total), font.clone(), theme.text_muted);
                        ui.painter().galley(
                            pos2(right - total.size().x, rect.center().y - total.size().y / 2.0),
                            total,
                            theme.text_muted,
                        );
                        let bar = Rect::from_min_max(
                            pos2(rect.left() + 164.0, rect.center().y - 3.0),
                            pos2((right - 94.0).max(rect.left() + 172.0), rect.center().y + 3.0),
                        );
                        ui.painter().rect_filled(
                            bar,
                            3.0,
                            Color32::from_black_alpha(if theme.dark { 56 } else { 15 }),
                        );
                        let ratio = (row.total / self.app_peak.max(1.0)).clamp(0.0, 1.0) as f32;
                        let fill = motion::follow(
                            ui.ctx(),
                            theme,
                            Id::unique(("dashboard-process-bar", &row.name)),
                            bar.width() * ratio,
                            theme.anim_fast as f32,
                            motion::linear,
                        );
                        ui.painter().rect_filled(
                            Rect::from_min_size(bar.min, vec2(fill, bar.height())),
                            3.0,
                            theme.accent,
                        );
                    }
                });
        });
        widgets::card_frame(theme, 14.0).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 32.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let items = bridge.routing_preset_options();
                    let active = bridge.active_routing_preset_id();
                    let current = items
                        .iter()
                        .position(|item| item["id"].as_str() == Some(&active))
                        .or_else(|| (!items.is_empty()).then(|| usize::from(items.len() > 1)));
                    let names: Vec<&str> = items
                        .iter()
                        .map(|item| item["name"].as_str().unwrap_or_default())
                        .collect();
                    let width = (ui.available_width() - 115.0).clamp(90.0, 260.0);
                    if let Some(index) =
                        widgets::combo(ui, theme, "dashboard-routing", width, &names, current, true)
                    {
                        if let Some(id) = items[index]["id"].as_str() {
                            bridge.apply_routing_preset_option(id);
                        }
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        widgets::section_title(ui, theme, &tr("Маршрутизация", &[]));
                    });
                },
            );
        });
    }
}

/// The local / system proxy / TUN segmented control (`ModeButton` row).
fn mode_switch(ui: &mut Ui, theme: &Theme, mode: &str, busy: bool) -> Option<&'static str> {
    let size = theme.font_small as f32 + 1.0;
    let entries = [
        ("local", tr("Локальный", &[])),
        ("system", tr("Сист. прокси", &[])),
        ("tun", "VPN (TUN)".to_string()),
    ];
    let widths: Vec<f32> = entries
        .iter()
        .map(|(key, label)| {
            let font = if *key == mode {
                semibold(size)
            } else {
                FontId::proportional(size)
            };
            widgets::galley(ui, label, font, theme.text).size().x.ceil() + 24.0
        })
        .collect();
    let total = widths.iter().sum::<f32>() + 2.0 * 2.0 + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 32.0), Sense::hover());
    let dim = if busy { 0.6 } else { 1.0 };
    widgets::bordered_rect(
        ui.painter(),
        rect,
        6.0,
        Color32::from_black_alpha(if theme.dark { 46 } else { 10 }).gamma_multiply(dim),
        theme.border_solid.gamma_multiply(dim),
    );
    let mut picked = None;
    let mut x = rect.left() + 2.0;
    for ((key, label), width) in entries.iter().zip(widths) {
        let button = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(width, 28.0));
        x += width + 2.0;
        let response = widgets::hit(ui, button, Id::unique(("dashboard-mode", *key)));
        let active = *key == mode;
        let fill = if active {
            theme.accent_soft
        } else if response.hovered() {
            theme.card_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(button, 4.0, fill.gamma_multiply(dim));
        let (font, color) = if active {
            (semibold(size), theme.text)
        } else {
            (FontId::proportional(size), theme.text_muted)
        };
        let text = widgets::galley(ui, label, font, color.gamma_multiply(dim));
        ui.painter()
            .galley(button.center() - text.size() / 2.0, text, color);
        if !busy && response.clicked() && !active {
            picked = Some(*key);
        }
    }
    picked
}

/// Two stacked single-line texts (name + muted caption) centred on `left_center`.
fn two_lines(
    ui: &Ui,
    theme: &Theme,
    left_center: egui::Pos2,
    width: f32,
    title: &str,
    caption: &str,
    color: Color32,
) {
    let title = widgets::elided(
        ui,
        title,
        FontId::proportional(theme.font_normal as f32),
        color,
        width,
    );
    let caption = widgets::elided(
        ui,
        caption,
        FontId::proportional(theme.font_small as f32),
        theme.text_muted,
        width,
    );
    let top = left_center.y - (title.size().y + caption.size().y) / 2.0;
    let caption_top = top + title.size().y;
    ui.painter().galley(pos2(left_center.x, top), title, color);
    ui.painter()
        .galley(pos2(left_center.x, caption_top), caption, theme.text_muted);
}

fn latency_chart(ui: &mut Ui, theme: &Theme, samples: &VecDeque<i64>) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    let max = samples.iter().copied().max().unwrap_or(1).max(1) as f32;
    let width = ((rect.width() - 60.0) / 16.0).max(1.0);
    for index in 0_usize..16 {
        let latency = index
            .checked_sub(16 - samples.len())
            .and_then(|slot| samples.get(slot));
        let height = latency.map_or(4.0, |ms| (18.0 * (0.25 + 0.75 * *ms as f32 / max)).max(4.0));
        let bar = egui::Rect::from_min_size(
            egui::pos2(
                rect.left() + (width + 4.0) * index as f32,
                rect.bottom() - height,
            ),
            vec2(width, height),
        );
        let color = if latency.is_none() {
            theme.divider
        } else if index == 15 {
            theme.accent
        } else {
            theme.accent.gamma_multiply(0.45)
        };
        ui.painter().rect_filled(bar, 2.0, color);
    }
}

fn speed(value: f64) -> String {
    if value < 1024.0 {
        format!("{value:.0} B/s")
    } else if value < 1024.0_f64.powi(2) {
        format!("{:.2} KB/s", value / 1024.0)
    } else if value < 1024.0_f64.powi(3) {
        format!("{:.2} MB/s", value / 1024.0_f64.powi(2))
    } else {
        format!("{:.2} GB/s", value / 1024.0_f64.powi(3))
    }
}

fn bytes(value: f64) -> String {
    if value < 1024.0 {
        format!("{value:.0} B")
    } else if value < 1024.0_f64.powi(2) {
        format!("{:.1} KB", value / 1024.0)
    } else if value < 1024.0_f64.powi(3) {
        format!("{:.1} MB", value / 1024.0_f64.powi(2))
    } else {
        format!("{:.2} GB", value / 1024.0_f64.powi(3))
    }
}

fn count_label(label: &str, count: usize) -> String {
    if count > 1 {
        tr(
            &format!("{label} ({{count}})"),
            &[("count", &count.to_string())],
        )
    } else {
        tr(label, &[])
    }
}

fn column(ui: &mut Ui, width: f32, content: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        content(ui);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_formats_rate_and_volume_at_binary_unit_boundaries() {
        assert_eq!(speed(1024.0), "1.00 KB/s");
        assert_eq!(bytes(1024.0), "1.0 KB");
        assert_eq!(bytes(1024.0_f64.powi(3)), "1.00 GB");
    }
}

/// `modeHelp` in `DashboardPage.qml`: a 460 px dialog with no content
/// padding; titles and texts carry their own 20 px insets, 10 px apart, the
/// texts muted at 1.25 line height, a divider between the two modes and a
/// single accent "Понятно" button. Returns whether it stays open.
fn mode_help(ctx: &egui::Context, theme: &Theme) -> bool {
    use crate::shell::fluent_dialog::{DialogAction, FluentDialog, body_text_spaced};
    let action = FluentDialog::new("dashboard-mode-help", tr("VPN (TUN) или системный прокси", &[]), 460.0)
        .ok(tr("Понятно", &[]))
        .no_cancel()
        // header bottomPadding 6 (the shared header has 2); `padding: 0`
        // leaves the style's `verticalPadding: 18` on top and bottom.
        .padding(egui::Margin { left: 20, right: 20, top: 22, bottom: 18 })
        .show(ctx, theme, |ui| {
            let strong = crate::shell::theme::semibold(theme.font_strong as f32);
            let body = FontId::proportional(theme.font_normal as f32);
            crate::shell::fluent_dialog::body_text(ui, &tr("Системный прокси", &[]), strong.clone(), theme.text);
            ui.add_space(10.0);
            body_text_spaced(ui, &tr("Работает с браузерами, мессенджерами и приложениями, которые используют настройки прокси Windows. Обычно запускается быстрее, но чаще не действует в играх и программах без поддержки прокси.", &[]), body.clone(), theme.text_muted, 1.25);
            ui.add_space(10.0);
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0.0, theme.divider);
            ui.add_space(10.0);
            crate::shell::fluent_dialog::body_text(ui, "VPN (TUN)", strong, theme.text);
            ui.add_space(10.0);
            body_text_spaced(ui, &tr("Перехватывает трафик почти всех приложений, включая игры, и применяет правила маршрутизации на уровне системы. Требует прав администратора и запускается немного дольше.", &[]), body, theme.text_muted, 1.25);
        });
    action == DialogAction::None
}
