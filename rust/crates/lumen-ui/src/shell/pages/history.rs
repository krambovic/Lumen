//! Traffic summary and tables from `HistoryPage.qml`.

use std::cmp::Ordering;

use egui::{
    Align, FontId, Frame, Label, Layout, Margin, RichText, ScrollArea, Sense, Stroke, Ui,
    vec2,
};
use egui_extras::{Column, TableBuilder};
use lumen_core::i18n::tr;
use serde_json::Value;

use super::PageContext;
use crate::shell::theme::{Theme, semibold};
use crate::view_models::history_helpers::{mode_label_for, route_label_for};

const PERIODS: [(i64, &str); 3] = [(7, "За 7 дней"), (30, "За 30 дней"), (3650, "За всё время")];
const ROW_HEIGHT: f32 = 34.0;
const TABLE_GAP: f32 = 12.0;

struct HistoryColumn {
    key: &'static str,
    title: &'static str,
    width: f32,
}

impl HistoryColumn {
    const fn new(key: &'static str, title: &'static str, width: f32) -> Self {
        Self { key, title, width }
    }

    fn layout(&self) -> Layout {
        if matches!(self.key, "durSec" | "downB" | "upB" | "route") {
            Layout::right_to_left(Align::Center)
        } else {
            Layout::left_to_right(Align::Center)
        }
    }

    fn cell(&self, row: &Value, theme: &Theme) -> RichText {
        let value = |key| row[key].as_str().unwrap_or_default();
        let (text, color) = match self.key {
            "ts" => (value("date").into(), theme.text),
            "mode" => {
                let color = if row["isTun"].as_bool() == Some(true) {
                    theme.accent
                } else {
                    theme.text
                };
                (tr(&mode_label_for(value("mode")), &[]), color)
            }
            "durSec" => (value("duration").into(), theme.text),
            "downB" => {
                let color = if row["hasDown"].as_bool() == Some(true) {
                    theme.success
                } else {
                    theme.text
                };
                (value("down").into(), color)
            }
            "upB" => (value("up").into(), theme.text),
            "route" => (tr(&route_label_for(value("route")), &[]), theme.text_muted),
            key => (value(key).into(), theme.text),
        };
        RichText::new(text).size(13.0).color(color)
    }
}

struct TableSpec {
    key: &'static str,
    title: &'static str,
    height: f32,
    columns: &'static [HistoryColumn],
}

const TABLES: [TableSpec; 3] = [
    TableSpec {
        key: "sessions",
        title: "Сессии",
        height: 250.0,
        columns: &[
            HistoryColumn::new("ts", "Начало", 140.0),
            HistoryColumn::new("node", "Сервер", 0.0),
            HistoryColumn::new("mode", "Режим", 150.0),
            HistoryColumn::new("durSec", "Длит.", 80.0),
            HistoryColumn::new("downB", "↓", 100.0),
            HistoryColumn::new("upB", "↑", 100.0),
        ],
    },
    TableSpec {
        key: "daily",
        title: "Трафик по дням",
        height: 180.0,
        columns: &[
            HistoryColumn::new("day", "День", 0.0),
            HistoryColumn::new("downB", "Загружено", 150.0),
            HistoryColumn::new("upB", "Отдано", 150.0),
        ],
    },
    TableSpec {
        key: "procs",
        title: "Трафик по процессам",
        height: 210.0,
        columns: &[
            HistoryColumn::new("exe", "Процесс", 0.0),
            HistoryColumn::new("downB", "Загружено", 140.0),
            HistoryColumn::new("upB", "Отдано", 140.0),
            HistoryColumn::new("route", "Маршрут", 110.0),
        ],
    },
];

#[derive(Default)]
enum Sort {
    #[default]
    Natural,
    Ascending(&'static str),
    Descending(&'static str),
}

impl Sort {
    fn cycle(&mut self, key: &'static str) {
        *self = match *self {
            Self::Ascending(current) if current == key => Self::Descending(key),
            Self::Descending(current) if current == key => Self::Natural,
            _ => Self::Ascending(key),
        };
    }

    fn arrow(&self, key: &str) -> &str {
        match *self {
            Self::Ascending(current) if current == key => "  ▲",
            Self::Descending(current) if current == key => "  ▼",
            _ => "",
        }
    }

    fn order(&self, rows: &[Value], order: &mut Vec<usize>) {
        order.clear();
        order.extend(0..rows.len());
        let (key, descending) = match *self {
            Self::Natural => return,
            Self::Ascending(key) => (key, false),
            Self::Descending(key) => (key, true),
        };
        order.sort_by(|&a, &b| {
            let a = &rows[a][key];
            let b = &rows[b][key];
            let result = match (a.as_i64(), b.as_i64()) {
                (Some(a), Some(b)) => a.cmp(&b),
                _ => compare_text(
                    a.as_str().unwrap_or_default(),
                    b.as_str().unwrap_or_default(),
                ),
            };
            if descending { result.reverse() } else { result }
        });
    }
}

fn compare_text(left: &str, right: &str) -> Ordering {
    #[cfg(windows)]
    if !left.is_empty() && !right.is_empty() {
        use windows::Win32::Globalization::{
            CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringEx,
        };
        use windows::core::PCWSTR;

        let left: Vec<_> = left.encode_utf16().collect();
        let right: Vec<_> = right.encode_utf16().collect();
        // A null locale name selects the user's default Windows locale.
        match unsafe {
            CompareStringEx(
                PCWSTR::null(),
                Default::default(),
                &left,
                &right,
                None,
                None,
                None,
            )
        } {
            CSTR_LESS_THAN => return Ordering::Less,
            CSTR_EQUAL => return Ordering::Equal,
            CSTR_GREATER_THAN => return Ordering::Greater,
            _ => {}
        }
    }
    left.cmp(right)
}

#[derive(Default)]
struct TableState {
    sort: Sort,
    order: Vec<usize>,
}

pub(super) struct HistoryPage {
    period: usize,
    snapshot: Option<Value>,
    tables: [TableState; TABLES.len()],
}

impl Default for HistoryPage {
    fn default() -> Self {
        Self {
            period: 1,
            snapshot: None,
            tables: Default::default(),
        }
    }
}

impl HistoryPage {
    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        ScrollArea::vertical()
            .id_salt("history-page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
                let previous_period = self.period;
                let (refresh, clear) = self.toolbar(ui, theme);
                if clear {
                    page.bridge.clear_traffic_history();
                }
                if self.snapshot.is_none() || refresh || clear || self.period != previous_period {
                    let data = page.bridge.history_data(PERIODS[self.period].0);
                    for (spec, state) in TABLES.iter().zip(&mut self.tables) {
                        state.sort.order(rows(&data, spec), &mut state.order);
                    }
                    self.snapshot = Some(data);
                }
                let data = self
                    .snapshot
                    .as_ref()
                    .expect("history snapshot is loaded on first render");
                render_summary(ui, theme, data);
                for (spec, state) in TABLES.iter().zip(&mut self.tables) {
                    render_table(ui, theme, spec, rows(data, spec), state);
                }
            });
    }

    fn toolbar(&mut self, ui: &mut Ui, theme: &Theme) -> (bool, bool) {
        let title = RichText::new(tr("История", &[])).font(semibold(theme.font_title as f32));
        let period = egui::ComboBox::from_id_salt("history-period").icon(crate::shell::widgets::combo_chevron)
            .selected_text(
                RichText::new(tr(PERIODS[self.period].1, &[])).size(theme.font_normal as f32),
            )
            .width(180.0)
            .icon(|ui, rect, visuals, _| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "\u{e70d}",
                    FontId::proportional(12.0),
                    visuals.text_color(),
                );
            });
        let height = theme.control_height as f32;
        let select_period = |ui: &mut Ui| {
            crate::shell::widgets::fading_combo(ui, |ui| period.show_ui(ui, |ui| {
                for (index, (_, label)) in PERIODS.iter().enumerate() {
                    ui.selectable_value(&mut self.period, index, tr(label, &[]));
                }
            }));
        };
        if ui.available_width() < 500.0 {
            ui.label(title);
            ui.horizontal_wrapped(|ui| {
                toolbar_style(ui, height);
                select_period(ui);
                (
                    toolbar_button(ui, theme, "\u{e72c}", "Обновить"),
                    toolbar_button(ui, theme, "\u{e74d}", "Сбросить"),
                )
            })
            .inner
        } else {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), height),
                Layout::right_to_left(Align::Center),
                |ui| {
                    toolbar_style(ui, height);
                    let clear = toolbar_button(ui, theme, "\u{e74d}", "Сбросить");
                    let refresh = toolbar_button(ui, theme, "\u{e72c}", "Обновить");
                    select_period(ui);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| ui.label(title));
                    (refresh, clear)
                },
            )
            .inner
        }
    }
}

fn toolbar_style(ui: &mut Ui, height: f32) {
    let spacing = ui.spacing_mut();
    spacing.interact_size.y = height;
    spacing.item_spacing.x = 6.0;
    spacing.button_padding = vec2(14.0, 0.0);
}

fn toolbar_button(ui: &mut Ui, theme: &Theme, glyph: &str, caption: &str) -> bool {
    ui.button(
        RichText::new(format!("{glyph}  {}", tr(caption, &[]))).size(theme.font_normal as f32),
    )
    .clicked()
}

fn rows<'a>(data: &'a Value, spec: &TableSpec) -> &'a [Value] {
    data[spec.key]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn card(theme: &Theme) -> Frame {
    Frame::new()
        .fill(theme.card)
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius as u8)
}

fn render_summary(ui: &mut Ui, theme: &Theme, data: &Value) {
    let down = data["summaryDown"].as_str().unwrap_or("0 B");
    let up = data["summaryUp"].as_str().unwrap_or("0 B");
    let sessions = data["summarySessions"].as_u64().unwrap_or(0).to_string();
    let tiles = [
        ("Загружено", down, theme.success),
        ("Отдано", up, theme.text),
        ("Сессий", sessions.as_str(), theme.text),
    ]
    .map(|(title, value, color)| (tr(title, &[]), value, color));
    let caption_font = FontId::proportional(theme.font_small as f32);
    let value_font = semibold(theme.font_title as f32);
    let preferred = tiles.each_ref().map(|(title, value, _)| {
        let measure = |text: &str, font| {
            ui.painter()
                .layout_no_wrap(text.to_owned(), font, theme.text)
                .size()
                .x
        };
        32.0 + measure(title, caption_font.clone()).max(measure(value, value_font.clone()))
    });
    let spacing = theme.spacing as f32;
    let width = (ui.available_width() - 2.0 * spacing).max(0.0);
    let scale = width / preferred.iter().sum::<f32>();
    let content_height = caption_font.size + value_font.size + 15.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spacing;
        for ((title, value, color), width) in tiles.into_iter().zip(preferred) {
            let tile_width = width * scale;
            ui.allocate_ui_with_layout(
                vec2(tile_width, content_height + 32.0),
                Layout::top_down(Align::Min),
                |ui| {
                    card(theme).inner_margin(15.0).show(ui, |ui| {
                        ui.set_width((tile_width - 32.0).max(0.0));
                        ui.set_min_height(content_height);
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.add(
                            Label::new(
                                RichText::new(title)
                                    .font(caption_font.clone())
                                    .color(theme.text_muted),
                            )
                            .truncate(),
                        );
                        ui.add(
                            Label::new(RichText::new(value).font(value_font.clone()).color(color))
                                .truncate(),
                        );
                    });
                },
            );
        }
    });
}

fn render_table(
    ui: &mut Ui,
    theme: &Theme,
    spec: &TableSpec,
    rows: &[Value],
    state: &mut TableState,
) {
    ui.add_space(4.0);
    ui.label(RichText::new(tr(spec.title, &[])).font(semibold(theme.font_strong as f32)));
    card(theme)
        .inner_margin(Margin {
            left: 13,
            right: 2,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(TABLE_GAP, 0.0);
            // Leave the scrollbar in the card's right inset, beside the last column.
            let columns_width =
                ui.available_width() - 11.0 - TABLE_GAP * (spec.columns.len() - 1) as f32;
            let fixed_width: f32 = spec.columns.iter().map(|column| column.width).sum();
            let scale = ((columns_width - 86.0) / fixed_width).clamp(0.0, 1.0);
            let header_height = 25.0 + theme.font_small as f32;
            let body_height = spec.height + 50.0 - header_height;
            let mut table = TableBuilder::new(ui)
                .id_salt(spec.key)
                .min_scrolled_height(body_height)
                .max_scroll_height(body_height)
                .auto_shrink([false, false]);
            for column in spec.columns {
                let width = if column.width == 0.0 {
                    Column::exact((columns_width - fixed_width * scale).max(0.0))
                } else {
                    Column::exact(column.width * scale)
                };
                table = table.column(width.clip(true));
            }
            let output = table
                .header(header_height, |mut header| {
                    for column in spec.columns {
                        header.col(|ui| {
                            ui.with_layout(column.layout(), |ui| {
                                let arrow = state.sort.arrow(column.key);
                                let color = if arrow.is_empty() {
                                    theme.text_muted
                                } else {
                                    theme.text
                                };
                                let title = format!("{}{arrow}", tr(column.title, &[]));
                                ui.add(
                                    Label::new(
                                        RichText::new(title)
                                            .font(semibold(theme.font_small as f32))
                                            .color(color),
                                    )
                                    .truncate(),
                                );
                                if ui
                                    .interact(
                                        ui.max_rect(),
                                        ui.scope_id().with(column.key),
                                        Sense::click(),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    state.sort.cycle(column.key);
                                    state.sort.order(rows, &mut state.order);
                                }
                            });
                        });
                    }
                })
                .body(|body| {
                    body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                        let data = &rows[state.order[row.index()]];
                        for column in spec.columns {
                            row.col(|ui| {
                                ui.with_layout(column.layout(), |ui| {
                                    ui.add(Label::new(column.cell(data, theme)).truncate());
                                });
                            });
                        }
                    });
                });
            let body = output.inner_rect;
            ui.painter().hline(
                body.left()..=body.right() - 11.0,
                body.top(),
                Stroke::new(1.0, theme.divider),
            );
            if rows.is_empty() {
                ui.painter().text(
                    body.center(),
                    egui::Align2::CENTER_CENTER,
                    tr("Нет записей", &[]),
                    FontId::proportional(theme.font_small as f32),
                    theme.text_faint,
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn history_sort_cycles_and_compares_raw_numbers() {
        let rows = [
            json!({"downB": 1024, "down": "1.0 KB"}),
            json!({"downB": 512, "down": "512 B"}),
            json!({"downB": 2048, "down": "2.0 KB"}),
        ];
        let mut sort = Sort::default();
        let mut order = Vec::new();
        sort.cycle("downB");
        sort.order(&rows, &mut order);
        assert_eq!(order, [1, 0, 2]);
        sort.cycle("downB");
        sort.order(&rows, &mut order);
        assert_eq!(order, [2, 0, 1]);
        sort.cycle("downB");
        sort.order(&rows, &mut order);
        assert_eq!(order, [0, 1, 2]);
    }

    #[test]
    fn history_summary_and_tables_fit_after_resize() {
        let ctx = egui::Context::default();
        let theme = Theme::resolve(&Default::default());
        crate::shell::theme::apply_fonts(&ctx, &theme);
        let data = json!({
            "summaryDown": "1.0 KB", "summarySessions": 1,
            "sessions": [{"date": "11.11.2026 18:42", "node": "A long server name for a narrow column",
                "mode": "singbox", "isTun": true, "down": "1.0 KB", "hasDown": true}]
        });
        let mut page = HistoryPage::default();
        for width in [780.0, 360.0] {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 1200.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                ui.set_width(width);
                page.toolbar(ui, &theme);
                render_summary(ui, &theme, &data);
                for (spec, state) in TABLES.iter().zip(&mut page.tables) {
                    let rows = rows(&data, spec);
                    state.sort.order(rows, &mut state.order);
                    render_table(ui, &theme, spec, rows, state);
                }
                assert!(ui.min_rect().width() <= width + 1.0);
            });
            output.textures_delta.clear();
        }
    }
}
