//! Archive options from `DiagnosticsExportDialog.qml`.

use crate::shell::wheel::GlideRoom;
use egui::{
    Align, Color32, Context, FontId, Frame, Id, Layout, Modal, Rect, ScrollArea, Sense, Stroke, Ui, Vec2,
    pos2, vec2,
};
use lumen_core::i18n::tr;
use serde_json::Value;

use crate::shell::theme::Theme;
use crate::shell::widgets::{ButtonKind, FluentButton};

const SECTIONS: [(&str, &str, &str); 7] = [
    (
        "errors",
        "Ошибки",
        "Файл errors.log — предупреждения и ошибки",
    ),
    (
        "core",
        "Логи ядра",
        "Файл core.log — xray / sing-box / zapret / TUN",
    ),
    (
        "app",
        "Логи приложения",
        "Файл app.log — события интерфейса и логики",
    ),
    (
        "traffic",
        "Логи трафика",
        "Файл traffic.log — скорость, пинг, авто-переключение",
    ),
    (
        "state",
        "Состояние приложения",
        "Настройки и список серверов (секреты вырезаны)",
    ),
    (
        "recent",
        "Недавние логи интерфейса",
        "Последние строки из текущей сессии",
    ),
    (
        "network",
        "Проверка сети",
        "Доступность IPv4/IPv6, состояние прокси и TUN",
    ),
];

pub(super) struct DiagnosticsExport {
    included: [bool; SECTIONS.len()],
    upload: Option<bool>,
}

pub(super) enum ExportResponse {
    Open,
    Cancel,
    Submit(Value),
}

impl DiagnosticsExport {
    pub(super) fn new() -> Self {
        Self {
            included: SECTIONS.map(|(key, _, _)| key != "network"),
            upload: None,
        }
    }

    fn request(&self, upload_enabled: bool) -> Value {
        let sections = SECTIONS
            .iter()
            .zip(self.included)
            .map(|((key, _, _), selected)| (key.to_string(), Value::Bool(selected)));
        Value::Object(
            sections
                .chain([(
                    "upload".into(),
                    Value::Bool(upload_enabled && self.upload.unwrap_or(true)),
                )])
                .collect(),
        )
    }

    /// `DiagnosticsExportDialog.qml`: a 540 px flyout with 20 px margins,
    /// 12 px between blocks, title + subtitle rows with a switch, a divider
    /// before the upload row and ghost / accent buttons.
    pub(super) fn show(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        upload_enabled: bool,
    ) -> ExportResponse {
        let screen = ctx.content_rect();
        let modal = Modal::new(Id::new("lumen-diagnostics-export"))
            // `Overlay.modal: Qt.rgba(0, 0, 0, 0.4)`.
            .backdrop_color(Color32::from_black_alpha(102))
            .frame(
                Frame::new()
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border))
                    .corner_radius(10),
            )
            .show(ctx, |ui| {
                ui.set_width(540.0_f32.min((screen.width() - 48.0).max(0.0)));
                let mut response = ExportResponse::Open;
                let max_height = (screen.height() - 50.0).max(60.0);
                ScrollArea::vertical()
                    .max_height(max_height)
                    // Grow to the content before scrolling, like the QML Popup.
                    .min_scrolled_height(max_height)
                    .show(ui, |ui| {
                        Frame::new().inner_margin(20).show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            let width = ui.available_width();
                            text_block(
                                ui,
                                &tr("Экспорт диагностики", &[]),
                                FontId::new(theme.font_title as f32, bold_family()),
                                theme.text,
                                width,
                            );
                            ui.add_space(12.0);
                            text_block(
                                ui,
                                &tr("Выберите, что включить в архив диагностики.", &[]),
                                FontId::proportional(theme.font_small as f32),
                                theme.text_muted,
                                width,
                            );
                            for ((_, title, subtitle), checked) in
                                SECTIONS.iter().zip(&mut self.included)
                            {
                                ui.add_space(12.0);
                                if let Some(value) = option(ui, theme, *checked, true, title, subtitle) {
                                    *checked = value;
                                }
                            }
                            ui.add_space(12.0);
                            let (line, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
                            ui.painter().rect_filled(line, 0.0, theme.border_solid);
                            ui.add_space(12.0);
                            // `enabled` and `defaultChecked` both follow the consent setting.
                            let upload = upload_enabled && self.upload.unwrap_or(true);
                            if let Some(value) = option(
                                ui,
                                theme,
                                upload,
                                upload_enabled,
                                "Отправить на сервер",
                                "Автоматически загрузить архив на сервер диагностики",
                            ) {
                                self.upload = Some(value);
                            }
                            // RowLayout margins 20 with topMargin 6 after the 12 px spacing.
                            ui.add_space(18.0);
                            let height = theme.control_height as f32;
                            ui.allocate_ui_with_layout(
                                vec2(width, height),
                                Layout::right_to_left(Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = 10.0;
                                    let export = tr("Экспортировать", &[]);
                                    let cancel = tr("Отмена", &[]);
                                    if FluentButton::new(ButtonKind::Accent, "", &export)
                                        .show(ui, theme)
                                        .clicked()
                                    {
                                        response = ExportResponse::Submit(self.request(upload_enabled));
                                    } else if FluentButton::new(ButtonKind::Ghost, "", &cancel)
                                        .show(ui, theme)
                                        .clicked()
                                    {
                                        response = ExportResponse::Cancel;
                                    }
                                },
                            );
                        });
                    }).glide_room(ui);
                response
            });
        if modal.should_close() {
            ExportResponse::Cancel
        } else {
            modal.inner
        }
    }
}

/// `font.bold: true` on the title.
fn bold_family() -> egui::FontFamily {
    crate::shell::theme::semibold_family()
}

fn text_block(ui: &mut Ui, text: &str, font: FontId, color: Color32, width: f32) {
    let galley = ui.painter().layout(text.to_owned(), font, color, width);
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
}

/// `OptRow`: title and wrapped subtitle (1 px apart) with the switch 12 px to
/// the right. A disabled row keeps its text colours; only the switch dims.
fn option(
    ui: &mut Ui,
    theme: &Theme,
    checked: bool,
    enabled: bool,
    title: &str,
    subtitle: &str,
) -> Option<bool> {
    const SWITCH: Vec2 = vec2(52.0, 20.0);
    let width = ui.available_width();
    let text_width = (width - 12.0 - SWITCH.x).max(0.0);
    let title = ui.painter().layout(
        tr(title, &[]),
        FontId::proportional(theme.font_normal as f32),
        theme.text,
        text_width,
    );
    let subtitle = ui.painter().layout(
        tr(subtitle, &[]),
        FontId::proportional(theme.font_small as f32),
        theme.text_muted,
        text_width,
    );
    let text_height = title.size().y + 1.0 + subtitle.size().y;
    let (row, _) = ui.allocate_exact_size(vec2(width, text_height.max(SWITCH.y)), Sense::hover());
    let top = row.center().y - text_height / 2.0;
    let title_height = title.size().y;
    ui.painter().galley(pos2(row.left(), top), title, theme.text);
    ui.painter()
        .galley(pos2(row.left(), top + title_height + 1.0), subtitle, theme.text_muted);
    let slot = Rect::from_min_size(pos2(row.right() - SWITCH.x, row.center().y - SWITCH.y / 2.0), SWITCH);
    // A child that is not allocated in the row: the row height stays the text's.
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(slot));
    super::settings::switch_widget(&mut child, theme, checked, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn diagnostics_options_keep_qml_defaults_and_require_upload_consent() {
        let mut options = DiagnosticsExport::new();
        assert_eq!(
            options.request(true),
            json!({"errors": true, "core": true, "app": true, "traffic": true,
                "state": true, "recent": true, "network": false, "upload": true}),
        );
        assert_eq!(options.request(false)["upload"], false);
        options.upload = Some(false);
        assert_eq!(options.request(true)["upload"], false);
    }

    #[test]
    fn diagnostics_dialog_closes_on_escape_without_submitting() {
        let ctx = Context::default();
        let theme = Theme::resolve(&Default::default());
        let mut options = DiagnosticsExport::new();
        let mut output = ctx.run_ui(Default::default(), |_| {
            assert!(matches!(
                options.show(&ctx, &theme, false),
                ExportResponse::Open
            ));
        });
        output.textures_delta.clear();
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut closed = false;
        let mut output = ctx.run_ui(input, |_| {
            closed |= matches!(options.show(&ctx, &theme, false), ExportResponse::Cancel);
        });
        output.textures_delta.clear();
        assert!(closed);
    }
}
