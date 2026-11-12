//! Project information and support dialogs from `AboutPage.qml`.

use egui::{Id, Modal, OpenUrl, RichText, ScrollArea, TextureHandle, Ui, Vec2};
use lumen_core::i18n::tr;

use super::PageContext;

pub(in crate::shell) const DONATION_URL: &str =
    "https://www.donationalerts.com/r/studiobebraedition";
pub(in crate::shell) const USDT_ADDRESS: &str = "TWHsuUDru4pXBcGpfeKfzymbkfajyFnb2s";
const QR: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../windows/xray_fluent/qml_app/qml/assets/usdt-trc20-qr.png"
));

#[derive(Clone, Copy)]
enum Dialog {
    Support,
    Usdt,
}

#[derive(Default)]
pub(super) struct AboutPage {
    dialog: Option<Dialog>,
    qr: Option<TextureHandle>,
}

impl AboutPage {
    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        let bridge = &mut *page.bridge;
        ScrollArea::vertical().id_salt("about-page").show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
            ui.label(RichText::new(bridge.app_name()).size(theme.font_title as f32).strong());
            ui.label(RichText::new(format!("v{}", bridge.app_version())).size(theme.font_small as f32).color(theme.text_faint));
            ui.separator();
            ui.label(tr("Lumen — самостоятельный Windows-клиент для VPN/TUN, системного прокси, маршрутизации, управления серверами и обхода DPI (DPI bypass) через zapret.", &[]));
            let description = tr("Проект предлагает графический интерфейс на базе QML с эффектами Mica/Acrylic и аппаратным ускорением рендеринга.", &[]).replace("QML", "egui");
            ui.label(RichText::new(description).color(theme.text_muted));
            ui.separator();
            ui.label(RichText::new(tr("Ссылки", &[])).size(theme.font_strong as f32).strong());
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("Telegram канал", &[])).clicked() { open_url(ui, "https://telegram.me/lumenkvn"); }
                if ui.button(tr("Рекомендуемый VPN", &[])).clicked() { open_url(ui, bridge.recommended_vpn_url()); }
                if ui.button(tr("Поддержать проект", &[])).clicked() { self.dialog = Some(Dialog::Support); }
                if ui.button(tr("Показать приветствие", &[])).clicked() { bridge.show_onboarding_again(); }
            });
        });
        let Some(dialog) = self.dialog else {
            return;
        };
        let modal = Modal::new(Id::unique("lumen-about-dialog")).show(page.ctx, |ui| {
            ui.set_width(390.0_f32.min((page.ctx.content_rect().width() - 48.0).max(0.0)));
            match dialog {
                Dialog::Support => {
                    ui.heading(tr("Поддержать проект", &[]));
                    ui.label(tr("Выберите удобный способ поддержки:", &[]));
                    if ui.button("DonationAlerts").clicked() {
                        self.dialog = None;
                        open_url(ui, DONATION_URL);
                    }
                    if ui.button("USDT (TRC20)").clicked() {
                        self.dialog = Some(Dialog::Usdt);
                    }
                }
                Dialog::Usdt => {
                    ui.heading("USDT (TRC20)");
                    if self.qr.is_none() {
                        self.qr = load_qr(page.ctx);
                    }
                    ui.vertical_centered(|ui| {
                        if let Some(texture) = &self.qr {
                            ui.image((texture.id(), Vec2::splat(ui.available_width().min(250.0))));
                        }
                        ui.add(egui::Label::new(USDT_ADDRESS).selectable(true).wrap());
                        if ui.button(tr("Копировать адрес", &[])).clicked()
                            && let Some(text) = bridge.copy_text(USDT_ADDRESS)
                        {
                            ui.ctx().copy_text(text);
                        }
                    });
                }
            }
            ui.button(tr("Закрыть", &[])).clicked()
        });
        if modal.inner || modal.should_close() {
            self.dialog = None;
        }
    }
}

pub(in crate::shell) fn open_url(ui: &Ui, url: &str) {
    ui.ctx().open_url(OpenUrl::new_tab(url));
}

fn load_qr(ctx: &egui::Context) -> Option<TextureHandle> {
    let image = image::load_from_memory_with_format(QR, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture("lumen-usdt-trc20", image, egui::TextureOptions::NEAREST))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_support_qr_loads_as_a_square_texture() {
        let texture = load_qr(&egui::Context::default()).unwrap();
        assert_eq!(texture.size()[0], texture.size()[1]);
        assert!(texture.size()[0] >= 200);
    }
}
