//! Log controls and selectable entries from `LogsPage.qml`.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use egui::{
    Align, Button, Color32, FontId, Frame, Layout, Margin, RichText, ScrollArea, Sense,
    Stroke, TextEdit, TextFormat, Ui, text::LayoutJob, vec2,
};
use lumen_core::{i18n::tr, log_utils::LogEntry};
use regex::Regex;

use super::PageContext;
use super::diagnostics_export::{DiagnosticsExport, ExportResponse};
use super::text_edit_menu;
use crate::shell::theme::{Theme, bold, semibold};

const LEVELS: [(&str, &str); 5] = [
    ("all", "Все"),
    ("error", "Ошибки"),
    ("warning", "Предупреждения"),
    ("success", "События"),
    ("info", "Работа"),
];
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(160);

pub(super) struct LogsPage {
    search: String,
    search_due: Option<Instant>,
    auto_scroll: bool,
    previous_count: Option<usize>,
    export: DiagnosticsExport,
    export_open: bool,
}

impl Default for LogsPage {
    fn default() -> Self {
        Self {
            search: String::new(),
            search_due: None,
            auto_scroll: true,
            previous_count: None,
            export: DiagnosticsExport::new(),
            export_open: false,
        }
    }
}

impl LogsPage {
    pub(super) fn on_hidden(&mut self) {
        self.previous_count = None;
    }

    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        let bridge = &mut *page.bridge;
        let spacing = ui.spacing_mut();
        spacing.item_spacing.y = theme.spacing as f32;
        spacing.button_padding = vec2(14.0, 0.0);
        ui.label(RichText::new(tr("Логи и диагностика", &[])).font(bold(theme.font_title as f32)));
        let control_height = theme.control_height as f32;
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), control_height),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if log_button(ui, theme, tr("Экспорт диагностики", &[]), true).clicked()
                {
                    self.export = DiagnosticsExport::new();
                    self.export_open = true;
                }
                if log_button(ui, theme, tr("Очистить", &[]), false).clicked() {
                    self.search.clear();
                    self.search_due = None;
                    bridge.log_filter.set_search_text("");
                    bridge.log_filter.set_level_filter("all");
                    bridge.clear_logs();
                }
                let (rect, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), control_height),
                    Sense::hover(),
                );
                ui.painter()
                    .rect_filled(rect, theme.radius_small as u8, theme.card);
                let edit = ui.place(
                    rect.shrink2(vec2(12.0, 0.0)),
                    |ui: &mut Ui| {
                        text_edit_menu::show(
                            ui,
                            TextEdit::singleline(&mut self.search)
                                .frame(Frame::NONE)
                                .margin(Margin::ZERO)
                                .align(egui::Align2::LEFT_CENTER)
                                .hint_text(RichText::new(tr("Фильтр логов", &[])).color(theme.text_faint)),
                            "log-filter",
                            true,
                            false,
                        )
                    },
                );
                let border = if edit.has_focus() {
                    theme.accent
                } else {
                    theme.border_solid
                };
                ui.painter().rect_stroke(
                    rect,
                    theme.radius_small as u8,
                    Stroke::new(1.0, border),
                    egui::StrokeKind::Inside,
                );
                if edit.changed() {
                    self.search_due = Some(Instant::now() + SEARCH_DEBOUNCE);
                }
            },
        );
        if let Some(due) = self.search_due {
            let now = Instant::now();
            if now >= due {
                bridge.log_filter.set_search_text(&self.search);
                self.search_due = None;
            } else {
                ui.ctx().request_repaint_after(due - now);
            }
        }

        let mut follow_now = false;
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), control_height),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add_space(3.0);
                follow_now = auto_scroll_switch(ui, theme, &mut self.auto_scroll);
                let captions = LEVELS.map(|(key, title)| (key, tr(title, &[])));
                let caption_width: f32 = captions
                    .iter()
                    .map(|(key, title)| {
                        ui.painter()
                            .layout_no_wrap(
                                title.clone(),
                                button_font(theme, bridge.log_filter.level == *key),
                                theme.text,
                            )
                            .size()
                            .x
                    })
                    .sum();
                let gaps = ui.spacing().item_spacing.x * (captions.len() - 1) as f32;
                let padding = ((ui.available_width() - caption_width - gaps)
                    / (2 * captions.len()) as f32)
                    .floor()
                    .clamp(4.0, 14.0);
                ui.with_layout(
                    Layout::left_to_right(Align::Center).with_main_wrap(true),
                    |ui| {
                        ui.spacing_mut().button_padding.x = padding;
                        for (key, title) in captions {
                            if log_button(ui, theme, title, bridge.log_filter.level == key)
                                .clicked()
                            {
                                bridge.log_filter.set_level_filter(key);
                            }
                        }
                    },
                );
            },
        );

        let entries: Vec<_> = bridge
            .log_model
            .lines()
            .iter()
            .filter(|entry| bridge.log_filter.filter_accepts(entry))
            .collect();
        let previous_count = self.previous_count.replace(entries.len());
        let scroll_to_end =
            self.auto_scroll && (follow_now || previous_count != Some(entries.len()));
        let action = Frame::new()
            .fill(theme.card)
            .stroke(Stroke::new(1.0, theme.border_solid))
            .corner_radius(theme.radius as u8)
            .inner_margin(theme.spacing_large as i8 - 1)
            .show(ui, |ui| {
                if entries.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new(tr("Логи пусты", &[])).color(theme.text_faint))
                    });
                    return None;
                }
                ScrollArea::vertical()
                    .id_salt("lumen-log-list")
                    .auto_shrink([false, false])
                    .stick_to_bottom(self.auto_scroll)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let mut action = None;
                        for (index, entry) in entries.iter().enumerate() {
                            action = render_entry(ui, theme, index, entry).or(action);
                        }
                        if scroll_to_end {
                            ui.scroll_to_cursor(Some(Align::BOTTOM));
                        }
                        action
                    })
                    .inner
            })
            .inner;
        if let Some(action) = action {
            bridge.run_toast_action(&action);
        }

        if self.export_open {
            match self
                .export
                .show(page.ctx, theme, bridge.diagnostics_upload_enabled())
            {
                ExportResponse::Open => {}
                ExportResponse::Cancel => self.export_open = false,
                ExportResponse::Submit(request) => {
                    self.export_open = false;
                    bridge.run_diagnostics_export(&request);
                }
            }
        }
    }
}

fn log_button(ui: &mut Ui, theme: &Theme, title: String, accent: bool) -> egui::Response {
    let color = if accent {
        theme.accent_text
    } else {
        theme.text
    };
    let text = RichText::new(title)
        .font(button_font(theme, accent))
        .color(color);
    let mut button = Button::new(text).min_size(vec2(0.0, theme.control_height as f32));
    if accent {
        button = button.fill(theme.accent).stroke(Stroke::NONE);
    }
    ui.add(button)
}

fn button_font(theme: &Theme, accent: bool) -> FontId {
    let size = theme.font_normal as f32;
    if accent {
        semibold(size)
    } else {
        FontId::proportional(size)
    }
}

fn auto_scroll_switch(ui: &mut Ui, theme: &Theme, checked: &mut bool) -> bool {
    let label = tr("Автопрокрутка", &[]);
    let (rect, mut response) = ui.allocate_exact_size(vec2(44.0, 20.0), Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::CheckBox, true, *checked, &label)
    });
    let fill = if *checked {
        theme.accent
    } else {
        theme.control_fill
    };
    let stroke = if *checked {
        Stroke::NONE
    } else {
        Stroke::new(1.0, theme.border_solid)
    };
    ui.painter()
        .rect(rect, 10.0, fill, stroke, egui::StrokeKind::Inside);
    ui.painter().circle_filled(
        egui::pos2(
            if *checked {
                rect.right() - 10.0
            } else {
                rect.left() + 10.0
            },
            rect.center().y,
        ),
        5.0,
        if *checked {
            theme.accent_text
        } else {
            theme.text_muted
        },
    );
    ui.add_space(8.0);
    ui.label(RichText::new(label).color(theme.text_muted));
    response.changed()
}

fn render_entry(ui: &mut Ui, theme: &Theme, index: usize, entry: &LogEntry) -> Option<String> {
    static PREFIX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:ERROR|WARNING|WARN|INFO|DEBUG|FATAL|PANIC|CRITICAL)\b[\s:\-]*")
            .unwrap()
    });

    let (level, color) = match entry.level.as_str() {
        "error" => ("ERROR", theme.danger),
        "warning" => ("WARNING", theme.warning),
        "success" => ("EVENT", theme.success),
        _ => ("INFO", theme.text_muted),
    };
    let mut text = LayoutJob::default();
    let format = |color| TextFormat {
        font_id: FontId::proportional(theme.font_normal as f32),
        color,
        ..Default::default()
    };
    text.append(
        &format!("{} {} {level}  ", entry.timestamp, entry.source),
        0.0,
        format(color),
    );
    text.append(&PREFIX.replace(&entry.message, ""), 0.0, format(theme.text));
    if !entry.details.is_empty() && entry.details != entry.message {
        text.append("\n", 0.0, format(theme.text));
        text.append(&PREFIX.replace(&entry.details, ""), 0.0, format(theme.text));
    }

    let text_height = theme.font_normal as f32 + 5.0;
    let row_height = if entry.action_id.is_empty() {
        text_height
    } else {
        text_height.max(theme.control_height as f32)
    };
    let row = Frame::new()
        .fill(if index % 2 == 0 {
            Color32::TRANSPARENT
        } else {
            theme.card
        })
        .corner_radius(theme.radius_small as u8)
        .inner_margin(Margin {
            left: 15,
            right: 10,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), row_height),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let action = !entry.action_id.is_empty()
                        && log_button(ui, theme, tr(&entry.action_label, &[]), false).clicked();
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), text_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            let mut body = text.text.as_str();
                            let mut layout = |ui: &Ui, _: &dyn egui::TextBuffer, width| {
                                let mut job = text.clone();
                                job.wrap.max_width = width;
                                ui.fonts_mut(|fonts| fonts.layout_job(job))
                            };
                            text_edit_menu::show(
                                ui,
                                TextEdit::multiline(&mut body)
                                    .frame(Frame::NONE)
                                    .margin(Margin::ZERO)
                                    .desired_rows(1)
                                    .desired_width(f32::INFINITY)
                                    .layouter(&mut layout),
                                ("log-entry", index, &entry.timestamp, &entry.message),
                                false,
                                false,
                            );
                        },
                    );
                    action.then(|| entry.action_id.clone())
                },
            )
            .inner
        });
    let rect = row.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(rect.left() + 4.0, rect.top() + 4.0),
            egui::pos2(rect.left() + 7.0, rect.bottom() - 4.0),
        ),
        2.0,
        color,
    );
    row.inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_core::log_utils::parse_log_line;

    #[test]
    fn log_rows_follow_text_height_in_a_tall_viewport() {
        let ctx = egui::Context::default();
        let theme = Theme::resolve(&Default::default());
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(480.0, 700.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let mut entry = parse_log_line("[APP] INFO Starting local diagnostics", None);
            let start = ui.next_widget_position().y;
            render_entry(ui, &theme, 0, &entry);
            let after_short = ui.next_widget_position().y;
            let short_height = after_short - start;
            assert!((25.0..=40.0).contains(&short_height));

            entry.details = "Additional information about the connection. ".repeat(5);
            render_entry(ui, &theme, 1, &entry);
            let long_height = ui.next_widget_position().y - after_short;
            assert!(long_height > short_height);
            assert!(long_height < 150.0);
        });
        output.textures_delta.clear();
    }
}
