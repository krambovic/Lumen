//! QR export dialog shared by server views.

use egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};
use lumen_core::i18n::tr;
use qrcode::{Color, EcLevel, QrCode};

use crate::shell::theme::Theme;
use crate::view_models::app_bridge::UiEvent;

#[derive(Default)]
pub(super) struct NodeQr {
    payload: Option<(String, String)>,
    image: Option<Result<TextureHandle, String>>,
}

impl NodeQr {
    pub(super) fn is_open(&self) -> bool {
        self.payload.is_some()
    }

    pub(super) fn on_event(&mut self, event: &UiEvent) {
        if let UiEvent::NodeQrReady { link, name } = event {
            self.payload = Some((link.clone(), name.clone()));
            self.image = None;
        }
    }

    pub(super) fn render(&mut self, ctx: &Context, theme: &Theme) {
        let Some((link, name)) = &self.payload else {
            return;
        };
        let image = self.image.get_or_insert_with(|| {
            qr_image(link)
                .map(|image| ctx.load_texture("server-qr", image, TextureOptions::NEAREST))
                .map_err(|error| error.to_string())
        });
        // qrDialog: FluentDialog, 360 px, a centred 280 px image and the
        // muted, centred hint 12 px below; only a "Закрыть" OK button.
        let title = if name.is_empty() {
            tr("QR-код сервера", &[])
        } else {
            format!("{}{}", tr("QR-код: ", &[]), name)
        };
        let action = crate::shell::fluent_dialog::FluentDialog::new("server-qr", title, 360.0)
            .ok(tr("Закрыть", &[]))
            .no_cancel()
            .show(ctx, theme, |ui| {
                let width = ui.available_width();
                match image {
                    Ok(texture) => {
                        let side = width.min(280.0);
                        let (row, _) = ui.allocate_exact_size(egui::vec2(width, side), egui::Sense::hover());
                        ui.painter().image(
                            texture.id(),
                            egui::Rect::from_center_size(row.center(), egui::Vec2::splat(side)),
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        ui.add_space(12.0);
                        let mut job = egui::text::LayoutJob::simple(
                            tr("Отсканируйте код в клиенте, чтобы импортировать сервер.", &[]),
                            egui::FontId::proportional(theme.font_small as f32),
                            theme.text_muted,
                            width,
                        );
                        job.halign = egui::Align::Center;
                        let galley = ui.painter().layout_job(job);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, galley.size().y), egui::Sense::hover());
                        ui.painter().galley(egui::pos2(rect.center().x, rect.top()), galley, theme.text_muted);
                    }
                    Err(error) => crate::shell::fluent_dialog::body_text(
                        ui,
                        &tr("Не удалось создать QR-код (слишком длинный конфиг?): {err}", &[("err", error)]),
                        egui::FontId::proportional(theme.font_normal as f32),
                        theme.danger,
                    ),
                }
            });
        if action != crate::shell::fluent_dialog::DialogAction::None {
            *self = Self::default();
        }
    }
}

fn qr_image(payload: &str) -> qrcode::types::QrResult<ColorImage> {
    let code = QrCode::with_error_correction_level(payload, EcLevel::L)?;
    const QUIET_ZONE: usize = 4;
    let width = code.width() + QUIET_ZONE * 2;
    let mut image = ColorImage::filled([width, width], Color32::WHITE);
    for y in 0..code.width() {
        for x in 0..code.width() {
            if code[(x, y)] == Color::Dark {
                image[(x + QUIET_ZONE, y + QUIET_ZONE)] = Color32::BLACK;
            }
        }
    }
    Ok(image)
}
