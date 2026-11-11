//! Log controls and selectable entries from `LogsPage.qml`.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use egui::{
    Align, Button, Color32, FontId, Frame, Label, Layout, Margin, RichText, ScrollArea, Stroke,
    TextEdit, TextFormat, Ui, text::LayoutJob,
};
use lumen_core::{i18n::tr, log_utils::LogEntry};
use regex::Regex;

use super::PageContext;
use super::diagnostics_export::{DiagnosticsExport, ExportResponse};
use crate::shell::theme::Theme;

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
        ui.label(
            RichText::new(tr("Логи и диагностика", &[]))
                .size(theme.font_title as f32)
                .strong(),
        );
        ui.horizontal_wrapped(|ui| {
            let search = TextEdit::singleline(&mut self.search)
                .hint_text(tr("Фильтр логов", &[]))
                .desired_width((ui.available_width() * 0.55).max(100.0));
            if ui.add(search).changed() {
                self.search_due = Some(Instant::now() + SEARCH_DEBOUNCE);
            }
            if ui.button(tr("Очистить", &[])).clicked() {
                self.search.clear();
                self.search_due = None;
                bridge.log_filter.set_search_text("");
                bridge.log_filter.set_level_filter("all");
                bridge.clear_logs();
            }
            let export =
                Button::new(RichText::new(tr("Экспорт диагностики", &[])).color(theme.accent_text))
                    .fill(theme.accent);
            if ui.add(export).clicked() {
                self.export_open = true;
            }
        });
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
        ui.horizontal_wrapped(|ui| {
            for (key, title) in LEVELS {
                if ui
                    .add(Button::selectable(
                        bridge.log_filter.level == key,
                        tr(title, &[]),
                    ))
                    .clicked()
                {
                    bridge.log_filter.set_level_filter(key);
                }
            }
            follow_now = ui
                .checkbox(&mut self.auto_scroll, tr("Автопрокрутка", &[]))
                .changed();
        });

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
            .corner_radius(theme.radius_small as u8)
            .inner_margin(4.0)
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
        &format!("{}  {}  {level}  ", entry.timestamp, entry.source),
        0.0,
        format(color),
    );
    text.append(&PREFIX.replace(&entry.message, ""), 0.0, format(theme.text));
    if !entry.details.is_empty() && entry.details != entry.message {
        text.append("\n", 0.0, format(theme.text));
        text.append(&PREFIX.replace(&entry.details, ""), 0.0, format(theme.text));
    }

    let row = Frame::new()
        .fill(if index % 2 == 0 {
            Color32::TRANSPARENT
        } else {
            theme.card
        })
        .corner_radius(theme.radius_small as u8)
        .inner_margin(Margin {
            left: 15,
            right: 8,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let action = !entry.action_id.is_empty()
                    && ui.button(tr(&entry.action_label, &[])).clicked();
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add(Label::new(text).wrap().selectable(true));
                    },
                );
                action.then(|| entry.action_id.clone())
            })
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
