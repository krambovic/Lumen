//! QR export dialog shared by server views.

use egui::{Color32, ColorImage, Context, Id, Modal, TextureHandle, TextureOptions};
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
        let modal = Modal::new(Id::unique("server-qr-modal")).show(ctx, |ui| {
            ui.set_width(320.0_f32.min((ctx.content_rect().width() - 48.0).max(0.0)));
            let title = if name.is_empty() {
                tr("QR-код сервера", &[])
            } else {
                format!("{}{}", tr("QR-код: ", &[]), name)
            };
            ui.heading(title);
            match image {
                Ok(texture) => ui.vertical_centered(|ui| {
                    let size = ui.available_width().min(280.0);
                    ui.image((texture.id(), egui::Vec2::splat(size)));
                    ui.label(tr(
                        "Отсканируйте код в клиенте, чтобы импортировать сервер.",
                        &[],
                    ));
                }),
                Err(error) => ui.vertical(|ui| {
                    ui.colored_label(
                        theme.danger,
                        tr(
                            "Не удалось создать QR-код (слишком длинный конфиг?): {err}",
                            &[("err", error)],
                        ),
                    );
                }),
            };
            ui.button(tr("Закрыть", &[])).clicked()
        });
        if modal.inner || modal.should_close() {
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
