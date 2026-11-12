//! Connection controls, server shortcuts and live metrics from `DashboardPage.qml`.

use std::collections::{HashSet, VecDeque};

use egui::{
    Button, ComboBox, Frame, Id, Key, Modal, Modifiers, RichText, ScrollArea, Stroke, TextEdit, Ui,
};
use lumen_core::i18n::tr;

use super::{PageContext, flag, node_editor::NodeEditor};
use crate::shell::theme::Theme;
use crate::view_models::AppBridge;

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
        ScrollArea::vertical()
            .id_salt("dashboard")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
                self.connection(ui, theme, bridge);
                if ui.available_width() >= 960.0 {
                    ui.columns(2, |columns| {
                        self.servers(&mut columns[0], theme, bridge);
                        self.stats(&mut columns[1], theme, bridge);
                    });
                } else {
                    self.servers(ui, theme, bridge);
                    self.stats(ui, theme, bridge);
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
        card(ui, theme, |ui| {
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
            ui.label(
                RichText::new(format!("{} · {}", tr(state, &[]), tr(mode_label, &[]))).color(
                    if connected {
                        theme.success
                    } else {
                        theme.text_muted
                    },
                ),
            );
            let name = if bridge.selected_node_name().is_empty() {
                tr("Сервер не выбран", &[])
            } else {
                bridge.selected_node_name().to_string()
            };
            ui.horizontal_wrapped(|ui| {
                flag::show(ui, bridge.selected_node_flag_country(), egui::vec2(26.0, 18.0));
                ui.label(RichText::new(name).size(theme.font_title as f32).strong());
                if busy {
                    ui.spinner();
                }
            });
            let caption = [
                bridge.selected_node_protocol(),
                bridge.selected_node_source(),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
            if !caption.is_empty() {
                ui.label(RichText::new(caption).color(theme.text_muted));
            }
            ui.horizontal_wrapped(|ui| {
                let key = if connected {
                    "Отключить"
                } else {
                    "Подключить"
                };
                let action = if connected {
                    Button::new(tr(key, &[]))
                } else {
                    Button::new(RichText::new(tr(key, &[])).color(theme.accent_text))
                        .fill(theme.accent)
                };
                if ui.add_enabled(!busy, action).clicked() {
                    bridge.toggle_connection();
                }
                for (key, label) in [
                    ("tun", "VPN (TUN)"),
                    ("local", "Локальный"),
                    ("system", "Сист. прокси"),
                ] {
                    if ui
                        .add_enabled(!busy, Button::selectable(mode == key, tr(label, &[])))
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
                if ui.button(tr("Справка", &[])).clicked() {
                    self.mode_help = true;
                }
            });
            let metrics = bridge.metrics();
            ui.separator();
            ui.columns(2, |columns| {
                for (ui, (caption, rate)) in columns
                    .iter_mut()
                    .zip([("Загрузка", metrics.down_bps), ("Выгрузка", metrics.up_bps)])
                {
                    ui.label(tr(caption, &[]));
                    let rate = if !connected {
                        "0 B/s".into()
                    } else if !metrics.traffic_available {
                        "—".into()
                    } else {
                        speed(rate)
                    };
                    ui.label(RichText::new(rate).size(theme.font_title as f32).strong());
                }
            });
        });
    }

    fn servers(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        section(ui, theme, "Серверы");
        card(ui, theme, |ui| {
            let search = ui.add(
                TextEdit::singleline(&mut self.query)
                    .desired_width(f32::INFINITY)
                    .hint_text(tr("Поиск по имени, протоколу, стране", &[])),
            );
            if search.changed() {
                bridge.set_dashboard_node_filter(&self.query);
            }
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
            let label = if current.is_empty() {
                tr("Все группы", &[])
            } else {
                current.clone()
            };
            ComboBox::from_id_salt("dashboard-groups")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current.is_empty(), tr("Все группы", &[]))
                        .clicked()
                    {
                        self.selection.clear();
                        bridge.set_node_filter("", "");
                    }
                    for group in groups {
                        if ui.selectable_label(group == current, &group).clicked() {
                            self.selection.clear();
                            bridge.set_node_filter(&group, "");
                        }
                    }
                });
            if ui
                .add_enabled(
                    !bridge.transition_busy(),
                    Button::new(tr("Быстрое подключение", &[])),
                )
                .on_hover_text(tr("Лучший пинг в группе", &[]))
                .clicked()
            {
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
            ScrollArea::vertical()
                .id_salt("dashboard-node-list")
                .max_height(345.0)
                .auto_shrink([false, true])
                .show_rows(ui, 37.0, ids.len(), |ui, range| {
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
                            ui.horizontal(|ui| {
                                flag::show(ui, &country, egui::vec2(22.0, 16.0));
                                let latency = if ping < 0 {
                                    "—".into()
                                } else {
                                    format!("{ping} ms")
                                };
                                let width = (ui.available_width() - 76.0).max(40.0);
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
                                ui.label(
                                    RichText::new(latency)
                                        .color(theme.ping_color((ping >= 0).then_some(ping))),
                                );
                            });
                        });
                },
            )
            .response;
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

    fn stats(&self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        ui.columns(2, |columns| {
            card(&mut columns[0], theme, |ui| {
                section(ui, theme, "Задержка");
                let current = bridge.metrics().latency_ms;
                let value = if bridge.connected() && current >= 0 {
                    format!("{current} ms")
                } else {
                    "--".into()
                };
                ui.label(RichText::new(value).size(theme.font_title as f32).strong());
                latency_chart(ui, theme, &self.latency);
            });
            card(&mut columns[1], theme, |ui| {
                section(ui, theme, "Сессия");
                ui.label(
                    RichText::new(bytes(bridge.session_traffic_total() as f64))
                        .size(theme.font_title as f32)
                        .strong(),
                );
                ui.label(format!(
                    "↓ {} · ↑ {} · {}{}",
                    bytes(bridge.session_download_total() as f64),
                    bytes(bridge.session_upload_total() as f64),
                    tr("Пик: ", &[]),
                    speed(self.peak)
                ));
            });
        });
        card(ui, theme, |ui| {
            section(ui, theme, "Трафик приложений");
            let rows = bridge.process_model.rows();
            if rows.is_empty() {
                ui.label(tr(
                    "Статистика появится, когда приложения начнут передавать данные",
                    &[],
                ));
            }
            ScrollArea::vertical()
                .id_salt("dashboard-process-traffic")
                .max_height(224.0)
                .show(ui, |ui| {
                    for row in rows {
                        let name = row
                            .name
                            .rsplit(['/', '\\'])
                            .next()
                            .filter(|name| !name.is_empty())
                            .map_or_else(|| tr("Приложение", &[]), str::to_string);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(name);
                            ui.label(RichText::new(bytes(row.total)).color(theme.text_muted));
                        });
                        ui.add(
                            egui::ProgressBar::new((row.total / self.app_peak.max(1.0)) as f32)
                                .desired_height(4.0)
                                .fill(theme.accent),
                        );
                    }
                });
        });
        card(ui, theme, |ui| {
            section(ui, theme, "Маршрутизация");
            let items = bridge.routing_preset_options();
            let active = bridge.active_routing_preset_id();
            let current = items
                .iter()
                .find(|item| item["id"].as_str() == Some(&active))
                .or_else(|| items.get(usize::from(items.len() > 1)));
            ComboBox::from_id_salt("dashboard-routing")
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
        });
    }
}

fn latency_chart(ui: &mut Ui, theme: &Theme, samples: &VecDeque<i64>) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 35.0), egui::Sense::hover());
    let max = samples.iter().copied().max().unwrap_or(1).max(1) as f32;
    let slot = rect.width() / 16.0;
    for (index, latency) in samples.iter().enumerate() {
        let left = rect.left() + slot * (16 - samples.len() + index) as f32;
        let top = rect.bottom() - rect.height() * *latency as f32 / max;
        let bar = egui::Rect::from_min_size(
            egui::pos2(left, top),
            egui::vec2((slot - 2.0).max(1.0), rect.bottom() - top),
        );
        ui.painter()
            .rect_filled(bar, 2.0, theme.ping_color(Some(*latency)));
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

fn card(ui: &mut Ui, theme: &Theme, content: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(theme.card)
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius as u8)
        .inner_margin(14.0)
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
