//! Archive options from `DiagnosticsExportDialog.qml`.

use egui::{Button, Context, Frame, Id, Modal, RichText, ScrollArea, Stroke, Ui};
use lumen_core::i18n::tr;
use serde_json::Value;

use crate::shell::theme::Theme;

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

    pub(super) fn show(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        upload_enabled: bool,
    ) -> ExportResponse {
        let modal = Modal::new(Id::new("lumen-diagnostics-export"))
            .frame(
                Frame::popup(&ctx.global_style())
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border)),
            )
            .show(ctx, |ui| {
                ui.set_width(500.0_f32.min((ctx.content_rect().width() - 64.0).max(0.0)));
                ui.label(
                    RichText::new(tr("Экспорт диагностики", &[]))
                        .size(theme.font_title as f32)
                        .strong(),
                );
                ui.label(
                    RichText::new(tr("Выберите, что включить в архив диагностики.", &[]))
                        .color(theme.text_muted),
                );
                ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 160.0).max(60.0))
                    .show(ui, |ui| {
                        for ((_, title, subtitle), checked) in
                            SECTIONS.iter().zip(&mut self.included)
                        {
                            option(ui, theme, checked, title, subtitle);
                        }
                        ui.separator();
                        ui.add_enabled_ui(upload_enabled, |ui| {
                            let mut upload = self.upload.unwrap_or(upload_enabled);
                            if option(
                                ui,
                                theme,
                                &mut upload,
                                "Отправить на сервер",
                                "Автоматически загрузить архив на сервер диагностики",
                            ) {
                                self.upload = Some(upload);
                            }
                        });
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let submit = Button::new(
                        RichText::new(tr("Экспортировать", &[])).color(theme.accent_text),
                    )
                    .fill(theme.accent);
                    if ui.add(submit).clicked() {
                        ExportResponse::Submit(self.request(upload_enabled))
                    } else if ui.button(tr("Отмена", &[])).clicked() {
                        ExportResponse::Cancel
                    } else {
                        ExportResponse::Open
                    }
                })
                .inner
            });
        if modal.should_close() {
            ExportResponse::Cancel
        } else {
            modal.inner
        }
    }
}

fn option(ui: &mut Ui, theme: &Theme, checked: &mut bool, title: &str, subtitle: &str) -> bool {
    let changed = ui.checkbox(checked, tr(title, &[])).changed();
    ui.label(
        RichText::new(tr(subtitle, &[]))
            .size(theme.font_small as f32)
            .color(theme.text_muted),
    );
    ui.add_space(4.0);
    changed
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
