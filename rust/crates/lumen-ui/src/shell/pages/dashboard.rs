//! Connection controls, server shortcuts and live metrics from `DashboardPage.qml`.

use std::collections::{HashSet, VecDeque};

use egui::{
    Align, Button, Color32, ComboBox, Frame, Id, Key, Layout, Modal, Modifiers, RichText,
    ScrollArea, Sense, Stroke, TextEdit, Ui, vec2,
};
use lumen_core::i18n::tr;

use super::{PageContext, flag, node_editor::NodeEditor};
use crate::shell::theme::Theme;
use crate::view_models::AppBridge;

const GAP: f32 = 16.0;
const SERVER_WIDTH: f32 = 420.0;
const HERO_HEIGHT: f32 = 150.0;

#[derive(Default)]
pub(super) struct DashboardPage {
    query: String,
    quick_hint: bool,
    selection: HashSet<String>,
    latency: VecDeque<i64>,
    peak: f64,
    app_peak: f64,
    mode_help: bool,
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
            let modal = Modal::new(Id::unique("dashboard-mode-help")).show(page.ctx, |ui| {
                ui.set_width(475.0_f32.min((page.ctx.content_rect().width() - 48.0).max(0.0)));
                section(ui, theme, "VPN (TUN) или системный прокси");
                section(ui, theme, "Системный прокси");
                ui.label(tr("Работает с браузерами, мессенджерами и приложениями, которые используют настройки прокси Windows. Обычно запускается быстрее, но чаще не действует в играх и программах без поддержки прокси.", &[]));
                section(ui, theme, "VPN (TUN)");
                ui.label(tr("Перехватывает трафик почти всех приложений, включая игры, и применяет правила маршрутизации на уровне системы. Требует прав администратора и запускается немного дольше.", &[]));
                ui.button(tr("Понятно", &[])).clicked()
            });
            if modal.inner || modal.should_close() {
                self.mode_help = false;
            }
        }
        self.editor.render(page.ctx, theme, bridge);
    }

    fn connection(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let show_rates = ui.available_width() >= 760.0;
        card(ui, theme, 19.0, |ui| {
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
            ui.set_min_height(HERO_HEIGHT - 40.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let (ring, _) = ui.allocate_exact_size(vec2(52.0, 64.0), Sense::hover());
                let color = if connected {
                    theme.success
                } else {
                    theme.text_faint
                };
                ui.painter()
                    .circle_stroke(ring.center(), 22.0, Stroke::new(4.0, color));
                ui.painter().circle_filled(ring.center(), 7.5, color);
                let rates_width = if show_rates { 218.0 } else { 0.0 };
                let label_width = (ui.available_width() - rates_width).max(0.0);
                column(ui, label_width, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        small(
                            theme,
                            format!("{} · {}", tr(state, &[]), tr(mode_label, &[])),
                        )
                        .color(if connected {
                            theme.success
                        } else {
                            theme.text_muted
                        }),
                    );
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        flag::show(ui, bridge.selected_node_flag_country(), vec2(32.0, 24.0));
                        ui.add(
                            egui::Label::new(
                                RichText::new(name)
                                    .size(theme.font_title as f32 + 2.0)
                                    .strong(),
                            )
                            .truncate(),
                        );
                        if busy {
                            ui.spinner();
                        }
                    });
                    if !caption.is_empty() {
                        ui.add(egui::Label::new(small(theme, caption)).truncate());
                    }
                });
                if show_rates {
                    ui.add_space(7.0);
                    for (label, rate) in [
                        ("↓ Загрузка", metrics.down_bps),
                        ("↑ Выгрузка", metrics.up_bps),
                    ] {
                        column(ui, 86.0, |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.add_space(31.0);
                            let (arrow, label) = label.split_once(' ').expect("speed label");
                            ui.label(small(theme, format!("{arrow} {}", tr(label, &[]))));
                            let rate = if !connected {
                                "0 B/s".into()
                            } else if metrics.traffic_available {
                                speed(rate)
                            } else {
                                "—".into()
                            };
                            big_value(ui, theme, &rate);
                        });
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.spacing_mut().button_padding.y = 0.0;
                let key = if connected {
                    "Отключить"
                } else {
                    "Подключить"
                };
                let action = if connected {
                    Button::new(format!("\u{f8ae}  {}", tr(key, &[]))).fill(theme.card_hover)
                } else {
                    Button::new(
                        RichText::new(format!("\u{e768}  {}", tr(key, &[])))
                            .color(theme.accent_text),
                    )
                    .fill(theme.accent)
                };
                if ui
                    .add_enabled(!busy, action.min_size(vec2(180.0, 32.0)))
                    .clicked()
                {
                    bridge.toggle_connection();
                }
                Frame::new()
                    .fill(Color32::from_black_alpha(if theme.dark { 46 } else { 10 }))
                    .stroke(Stroke::new(1.0, theme.border_solid))
                    .corner_radius(6)
                    .inner_margin(1)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (key, label) in [
                                ("local", "Локальный"),
                                ("system", "Сист. прокси"),
                                ("tun", "VPN (TUN)"),
                            ] {
                                if ui
                                    .add_enabled(
                                        !busy,
                                        Button::new(tr(label, &[]))
                                            .fill(if mode == key {
                                                theme.accent_soft
                                            } else {
                                                Color32::TRANSPARENT
                                            })
                                            .stroke(Stroke::NONE)
                                            .corner_radius(4)
                                            .min_size(vec2(89.0, 28.0)),
                                    )
                                    .clicked()
                                    && key != mode
                                {
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
                            }
                        })
                    });
                if ui
                    .add_sized([32.0, 32.0], Button::new("\u{e946}"))
                    .on_hover_text(tr("Справка", &[]))
                    .clicked()
                {
                    self.mode_help = true;
                }
            });
        });
    }

    fn servers(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, height: f32) {
        card(ui, theme, 11.0, |ui| {
            let inner_height = height - 24.0;
            let top = ui.cursor().top();
            ui.set_min_height(inner_height);
            section(ui, theme, "Серверы");
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
            ScrollArea::horizontal()
                .id_salt("dashboard-groups")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.add_space(5.0);
                        for (group, label) in std::iter::once(("", tr("Все", &[])))
                            .chain(groups.iter().map(|name| (name.as_str(), name.clone())))
                        {
                            let selected = group == current;
                            let label = RichText::new(label).color(if selected {
                                theme.text
                            } else {
                                theme.text_muted
                            });
                            let tab = ui.add(
                                Button::new(if selected { label.strong() } else { label })
                                    .frame(false)
                                    .min_size(vec2(32.0, 34.0)),
                            );
                            if selected {
                                let center = tab.rect.center().x;
                                let y = tab.rect.bottom() - 3.0;
                                ui.painter().line_segment(
                                    [egui::pos2(center - 9.0, y), egui::pos2(center + 9.0, y)],
                                    Stroke::new(2.0, theme.accent),
                                );
                            }
                            if tab.clicked() {
                                self.selection.clear();
                                bridge.set_node_filter(group, "");
                            }
                            ui.add_space(12.0);
                        }
                    });
                });
            ui.separator();
            let (search, cleared) = server_search(ui, theme, &mut self.query);
            if search.changed() || cleared {
                self.selection.clear();
                bridge.set_dashboard_node_filter(&self.query);
            }
            let quick = ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt("quick-connect")
                    .sense(Sense::click()),
                |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add_space(9.0);
                        ui.label(RichText::new("\u{e945}").color(theme.accent));
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.label(tr("Быстрое подключение", &[]));
                            ui.label(small(theme, tr("Лучший пинг в группе", &[])));
                        });
                    });
                },
            );
            if !bridge.transition_busy() && quick.response.clicked() {
                let id = bridge.quick_connect_node_id();
                self.quick_hint = id.is_empty();
                if !self.quick_hint {
                    bridge.select_node(&id);
                    if !bridge.connected() && !bridge.transition_busy() {
                        bridge.toggle_connection();
                    }
                }
            }
            if self.quick_hint {
                ui.label(tr(
                    "Нет серверов с измеренным пингом — проверьте пинг на странице «Серверы»",
                    &[],
                ));
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
            if ids.is_empty() {
                ui.label(tr("Серверов нет — импортируйте ссылку или подписку", &[]));
            }
            let list_height = (inner_height - (ui.cursor().top() - top)).max(0.0);
            ScrollArea::vertical()
                .id_salt("dashboard-node-list")
                .max_height(list_height)
                .auto_shrink([false, false])
                .show_rows(ui, 44.0, ids.len(), |ui, range| {
                    for id in &ids[range] {
                        self.server_row(ui, theme, bridge, id);
                    }
                });
        });
    }

    fn server_row(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, id: &str) {
        let Ok(index) = usize::try_from(bridge.node_model.index_of_id(id)) else {
            return;
        };
        let model = &bridge.node_model;
        let active = model.selected_at(index);
        let picked = self.selection.contains(id);
        let ping = model.ping_at(index);
        let selected = active || picked;
        let enabled = model.runtime_supported_at(index);
        let name = model.name_at(index);
        let sub = [model.protocol_at(index), model.group_at(index)]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let country = model.country_at(index);
        let row = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .id_salt(id)
                    .sense(egui::Sense::click()),
                |ui| {
                    Frame::new()
                        .fill(if selected {
                            theme.accent_soft
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .corner_radius(theme.radius_small as u8)
                        .inner_margin(3.0)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.allocate_ui_with_layout(
                                vec2(ui.available_width(), 37.0),
                                Layout::left_to_right(Align::Center),
                                |ui| {
                                    ui.add_space(6.0);
                                    flag::show(ui, &country, egui::vec2(22.0, 16.0));
                                    let latency = if ping < 0 {
                                        "—".into()
                                    } else {
                                        format!("{ping} ms")
                                    };
                                    let width = (ui.available_width() - 55.0).max(40.0);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(width, 28.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            ui.add(
                                                egui::Label::new(RichText::new(&name).color(
                                                    if enabled {
                                                        theme.text
                                                    } else {
                                                        theme.text_muted
                                                    },
                                                ))
                                                .truncate(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(sub)
                                                        .size(theme.font_small as f32)
                                                        .color(theme.text_muted),
                                                )
                                                .truncate(),
                                            );
                                        },
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.add_space(7.0);
                                        ui.label(
                                            small(theme, latency).color(
                                                theme.ping_color((ping >= 0).then_some(ping)),
                                            ),
                                        );
                                    });
                                },
                            );
                        });
                },
            )
            .response;
        if selected {
            let center = row.rect.left_center();
            ui.painter().line_segment(
                [center - vec2(0.0, 8.0), center + vec2(0.0, 8.0)],
                Stroke::new(2.0, theme.accent),
            );
        }
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
        row.context_menu(|ui| self.server_menu(ui, bridge, id));
    }

    fn server_menu(&mut self, ui: &mut Ui, bridge: &mut AppBridge, id: &str) {
        let multi = self.selection.len() > 1 && self.selection.contains(id);
        let targets = if multi {
            self.selection.clone()
        } else {
            HashSet::from([id.into()])
        };
        for (label, action) in [
            ("Установить как активный", 0),
            ("Редактировать", 1),
            ("Копировать ссылку", 2),
            ("Экспорт в QR-код", 3),
        ] {
            if ui
                .add_enabled(!multi, Button::new(tr(label, &[])))
                .clicked()
            {
                match action {
                    0 => bridge.select_node(id),
                    1 => self.editor.edit(bridge, id),
                    2 => {
                        if let Some(link) = bridge.copy_node_link(id) {
                            ui.ctx().copy_text(link);
                        }
                    }
                    _ => bridge.show_node_qr(id),
                }
                ui.close();
            }
        }
        ui.separator();
        for (label, action) in [
            ("Пинг — способ из настроек", 0),
            ("Тест TCP ping", 1),
            ("Тест HTTP GET", 2),
            ("Тест реальной задержки", 3),
        ] {
            if ui.button(count_label(label, targets.len())).clicked() {
                match action {
                    0 => bridge.ping_nodes(Some(&targets)),
                    1 => bridge.tcping_nodes(Some(&targets)),
                    2 => bridge.http_get_nodes(Some(&targets)),
                    _ => bridge.real_delay_nodes(Some(&targets)),
                }
                ui.close();
            }
        }
        ui.separator();
        for (label, direction) in [("В начало списка", "top"), ("В конец списка", "bottom")]
        {
            if ui
                .add_enabled(!multi, Button::new(tr(label, &[])))
                .clicked()
            {
                bridge.reorder_node(id, direction);
                ui.close();
            }
        }
        if ui.button(count_label("Удалить", targets.len())).clicked() {
            bridge.delete_nodes(&targets.into_iter().collect::<Vec<_>>());
            self.selection.clear();
            ui.close();
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
        let latency = |ui: &mut Ui| {
            card(ui, theme, 11.0, |ui| {
                ui.set_min_height(72.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                section(ui, theme, "Задержка");
                let current = bridge.metrics().latency_ms;
                let value = if bridge.connected() && current >= 0 {
                    format!("{current} ms")
                } else {
                    "-- ms".into()
                };
                big_value(ui, theme, &value);
                latency_chart(ui, theme, &self.latency);
            });
        };
        let session = |ui: &mut Ui| {
            card(ui, theme, 11.0, |ui| {
                ui.set_min_height(72.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                section(ui, theme, "Сессия");
                big_value(ui, theme, &bytes(bridge.session_traffic_total() as f64));
                ui.label(small(
                    theme,
                    format!(
                        "↓ {} · ↑ {} · {}{}",
                        bytes(bridge.session_download_total() as f64),
                        bytes(bridge.session_upload_total() as f64),
                        tr("Пик: ", &[]),
                        speed(self.peak)
                    ),
                ));
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
            |height| (height - 96.0 - 60.0 - 2.0 * GAP).max(150.0),
        );
        card(ui, theme, 15.0, |ui| {
            ui.set_min_height(traffic_height - 32.0);
            section(ui, theme, "Трафик приложений");
            if rows.is_empty() {
                ui.allocate_ui_with_layout(vec2(ui.available_width(), traffic_height - 68.0),
                    Layout::left_to_right(Align::Center), |ui| {
                    ui.label(small(theme, tr("Статистика появится, когда приложения начнут передавать данные", &[]))
                        .color(theme.text_faint));
                });
                return;
            }
            ScrollArea::vertical()
                .id_salt("dashboard-process-traffic")
                .max_height(traffic_height - 64.0)
                .show_rows(ui, 26.0, rows.len(), |ui, range| {
                    for row in &rows[range] {
                        let name = row.name.rsplit(['/', '\\']).next().unwrap_or(&row.name);
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), 26.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 14.0;
                                let name_width = (ui.available_width() * 0.28).min(150.0);
                                ui.add_sized([name_width, 26.0], egui::Label::new(name).truncate());
                                let bar_width = (ui.available_width() - 94.0).max(8.0);
                                let (bar, _) =
                                    ui.allocate_exact_size(vec2(bar_width, 6.0), Sense::hover());
                                ui.painter().rect_filled(
                                    bar,
                                    3.0,
                                    Color32::from_black_alpha(if theme.dark { 56 } else { 15 }),
                                );
                                let ratio =
                                    (row.total / self.app_peak.max(1.0)).clamp(0.0, 1.0) as f32;
                                let fill = egui::Rect::from_min_size(
                                    bar.min,
                                    vec2(bar.width() * ratio, bar.height()),
                                );
                                ui.painter().rect_filled(fill, 3.0, theme.accent);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(small(theme, bytes(row.total)));
                                });
                            },
                        );
                    }
                });
        });
        card(ui, theme, 13.0, |ui| {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 32.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    let items = bridge.routing_preset_options();
                    let active = bridge.active_routing_preset_id();
                    let current = items
                        .iter()
                        .find(|item| item["id"].as_str() == Some(&active))
                        .or_else(|| items.get(usize::from(items.len() > 1)));
                    ComboBox::from_id_salt("dashboard-routing")
                        .width((ui.available_width() - 115.0).clamp(90.0, 260.0))
                        .selected_text(
                            current
                                .and_then(|item| item["name"].as_str())
                                .unwrap_or_default(),
                        )
                        .show_ui(ui, |ui| {
                            for item in &items {
                                let id = item["id"].as_str().unwrap_or_default();
                                if ui
                                    .selectable_label(
                                        id == active,
                                        item["name"].as_str().unwrap_or_default(),
                                    )
                                    .clicked()
                                {
                                    bridge.apply_routing_preset_option(id);
                                }
                            }
                        });
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        section(ui, theme, "Маршрутизация")
                    });
                },
            );
        });
    }
}

fn server_search(ui: &mut Ui, theme: &Theme, query: &mut String) -> (egui::Response, bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, theme.radius_small as f32, theme.control_fill);
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "\u{e721}",
        egui::FontId::proportional(14.0),
        theme.text_faint,
    );
    let edit_rect = egui::Rect::from_min_max(
        rect.min + vec2(34.0, 0.0),
        rect.max - vec2(if query.is_empty() { 10.0 } else { 32.0 }, 0.0),
    );
    let response = ui.place(
        edit_rect,
        TextEdit::singleline(query)
            .frame(Frame::NONE)
            .vertical_align(Align::Center)
            .margin(egui::Margin::ZERO)
            .hint_text(
                RichText::new(tr("Поиск по имени, протоколу, стране", &[])).color(theme.text_faint),
            ),
    );
    let cleared = !query.is_empty()
        && ui
            .place(
                egui::Rect::from_min_size(
                    egui::pos2(rect.right() - 32.0, rect.top()),
                    vec2(32.0, 32.0),
                ),
                Button::new("\u{e711}").frame(false),
            )
            .clicked();
    if cleared {
        query.clear();
        response.request_focus();
    }
    let border = if response.has_focus() {
        theme.accent
    } else {
        theme.border_solid
    };
    ui.painter().rect_stroke(
        rect,
        theme.radius_small as f32,
        Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    (response, cleared)
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

fn section(ui: &mut Ui, theme: &Theme, title: &str) {
    ui.label(
        RichText::new(tr(title, &[]))
            .size(theme.font_strong as f32)
            .strong(),
    );
}

fn small(theme: &Theme, text: impl Into<String>) -> RichText {
    RichText::new(text)
        .size(theme.font_small as f32)
        .color(theme.text_muted)
}

fn big_value(ui: &mut Ui, theme: &Theme, text: &str) {
    let (value, unit) = text.split_once(' ').unwrap_or((text, ""));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        ui.label(RichText::new(value).size(theme.font_title as f32).strong());
        ui.label(small(theme, unit));
    });
}

fn column(ui: &mut Ui, width: f32, content: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        content(ui);
    });
}

fn card(ui: &mut Ui, theme: &Theme, padding: f32, content: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(theme.card)
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius as u8)
        .inner_margin(padding)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 7.0;
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
