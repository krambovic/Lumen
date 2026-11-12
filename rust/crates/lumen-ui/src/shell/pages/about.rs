//! Project information and support dialogs from `AboutPage.qml`.

use egui::{
    Align, Color32, Frame, Id, Layout, Modal, OpenUrl, RichText, ScrollArea, Stroke, TextureHandle,
    Ui, Vec2, vec2,
};
use lumen_core::i18n::tr;

use super::PageContext;
use super::fluent_button;
use crate::shell::theme::{Theme, semibold};

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
        ScrollArea::vertical().id_salt("about-page").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 16.0;
            ui.label(RichText::new(bridge.app_name()).font(semibold(theme.font_title as f32)));
            ui.label(RichText::new(format!("v{}", bridge.app_version())).size(theme.font_small as f32).color(theme.text_faint));
            divider(ui, theme);
            ui.label(RichText::new(tr("Lumen — самостоятельный Windows-клиент для VPN/TUN, системного прокси, маршрутизации, управления серверами и обхода DPI (DPI bypass) через zapret.", &[])).size(15.0));
            let description = tr("Проект предлагает графический интерфейс на базе QML с эффектами Mica/Acrylic и аппаратным ускорением рендеринга.", &[]).replace("QML", "egui");
            ui.label(RichText::new(description).color(theme.text_muted));
            divider(ui, theme);
            ui.label(RichText::new(tr("Ссылки", &[])).font(semibold(theme.font_strong as f32)));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                fluent_button::style(ui);
                if ui.add(fluent_button::button(theme, "\u{e724}", &tr("Telegram канал", &[]), true)).clicked() { open_url(ui, "https://telegram.me/lumenkvn"); }
                if ui.add(fluent_button::button(theme, "\u{e734}", &tr("Рекомендуемый VPN", &[]), false)).clicked() { open_url(ui, bridge.recommended_vpn_url()); }
                if ui.add(fluent_button::button(theme, "\u{e8f1}", &tr("Поддержать проект", &[]), false)).clicked() { self.dialog = Some(Dialog::Support); }
                if ui.add(fluent_button::button(theme, "\u{e80f}", &tr("Показать приветствие", &[]), false)).clicked() { bridge.show_onboarding_again(); }
            });
        });
        let Some(dialog) = self.dialog else {
            return;
        };
        let width: f32 = match dialog {
            Dialog::Support => 430.0,
            Dialog::Usdt => 420.0,
        };
        let modal = Modal::new(Id::unique("lumen-about-dialog"))
            .backdrop_color(Color32::from_black_alpha(115))
            .frame(
                Frame::new()
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border))
                    .corner_radius(10.0)
                    .inner_margin(19.0),
            )
            .show(page.ctx, |ui| {
                ui.set_width((width - 40.0).min((page.ctx.content_rect().width() - 64.0).max(0.0)));
                ui.spacing_mut().item_spacing.y = 12.0;
                fluent_button::style(ui);
                let title = match dialog {
                    Dialog::Support => tr("Поддержать проект", &[]),
                    Dialog::Usdt => "USDT (TRC20)".into(),
                };
                ui.label(RichText::new(title).font(semibold(theme.font_strong as f32)));
                match dialog {
                    Dialog::Support => {
                        ui.label(
                            RichText::new(tr("Выберите удобный способ поддержки:", &[]))
                                .color(theme.text_muted),
                        );
                        let size = vec2(ui.available_width(), theme.control_height as f32);
                        if ui
                            .add_sized(
                                size,
                                fluent_button::button(theme, "\u{e8f1}", "DonationAlerts", true),
                            )
                            .clicked()
                        {
                            self.dialog = None;
                            open_url(ui, DONATION_URL);
                        }
                        if ui
                            .add_sized(
                                size,
                                fluent_button::button(theme, "\u{e8c7}", "USDT (TRC20)", false),
                            )
                            .clicked()
                        {
                            self.dialog = Some(Dialog::Usdt);
                        }
                    }
                    Dialog::Usdt => {
                        if self.qr.is_none() {
                            self.qr = load_qr(page.ctx);
                        }
                        ui.vertical_centered(|ui| {
                            if let Some(texture) = &self.qr {
                                let size =
                                (page.ctx.content_rect().height() - 228.0).clamp(64.0, 250.0);
                                ui.image((
                                    texture.id(),
                                    Vec2::splat(ui.available_width().min(size)),
                                ));
                            }
                            let mut address = USDT_ADDRESS;
                            ui.add_sized(
                                vec2(ui.available_width(), theme.control_height as f32),
                                egui::TextEdit::singleline(&mut address)
                                    .align(egui::Align2::CENTER_CENTER),
                            );
                            if ui
                                .add(fluent_button::button(
                                    theme,
                                    "\u{e8c8}",
                                    &tr("Копировать адрес", &[]),
                                    true,
                                ))
                                .clicked()
                                && let Some(text) = bridge.copy_text(USDT_ADDRESS)
                            {
                                ui.ctx().copy_text(text);
                            }
                        });
                    }
                }
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), theme.control_height as f32),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        ui.add(fluent_button::button(theme, "", &tr("Закрыть", &[]), false))
                            .clicked()
                    },
                )
                .inner
            });
        if modal.inner || modal.should_close() {
            self.dialog = None;
        }
    }
}

fn divider(ui: &mut Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme.divider);
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
